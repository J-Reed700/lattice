import { fetchCorpusShape } from '@/features/corpus-shape/hooks/useCorpusShapeQuery';
import { composeIngestSummary } from '@/features/files/components/IngestHub/ingestSummary';
import {
  type BatchImportItem,
  type BatchUrlImportResults,
  useBatchUrlImportStore,
} from '@/features/files/stores/batchUrlImportStore';
import { useFileBrowserStore } from '@/features/files/stores/fileBrowserStore';
import type { ApiClient } from '@/lib/api';
import { toast } from '@/stores/toastStore';
import type { CorpusShapeDto } from '@/types';
import type { BatchJobItem } from '@/types/api/batch';
import { listAllBatchJobs } from '@/utils/batchHistory';
import { cancelBatchJob, getBatchJobStatus, startBatchUrlImport } from '@/utils/batchImport';

const BATCH_POLL_INTERVAL_MS = 800;
const BATCH_POLL_TIMEOUT_MS = 180000;
const MAX_CONSECUTIVE_POLL_ERRORS = 5;
const ACTIVE_JOB_STATUSES = new Set(['pending', 'running', 'processing', 'in_progress']);

function formatInvokeError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && 'message' in err) {
    return String((err as { message?: string }).message);
  }
  try {
    return JSON.stringify(err);
  } catch {
    return "Couldn't fetch preview";
  }
}

function isValidUrl(text: string): boolean {
  try {
    const url = new URL(text);
    return ['http:', 'https:'].includes(url.protocol);
  } catch {
    return false;
  }
}

function normalizeUrl(text: string): string | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  if (isValidUrl(trimmed)) return trimmed;
  if (/^[a-z0-9.-]+\.[a-z]{2,}/i.test(trimmed)) {
    const candidate = `https://${trimmed}`;
    return isValidUrl(candidate) ? candidate : null;
  }
  return null;
}

function extractUrls(text: string): string[] {
  const lines = text.split('\n');
  const urls = new Set<string>();

  for (const line of lines) {
    const normalized = normalizeUrl(line);
    if (normalized) {
      urls.add(normalized);
    }
  }

  return Array.from(urls);
}

/** Reset library filters and navigate to `/files` without requiring router context. */
export const openLibraryRoot = (): void => {
  const library = useFileBrowserStore.getState();
  library.setScope({ kind: 'all' });
  library.setSearchQuery('');
  library.setFilterByType(null);
  library.setFilterBySource('all');
  library.clearContentSearch();
  window.history.pushState({}, '', '/files');
  window.dispatchEvent(new PopStateEvent('popstate'));
};

async function loadCorpusShapeSafe(api: ApiClient): Promise<CorpusShapeDto | null> {
  try {
    return await fetchCorpusShape(api);
  } catch {
    return null;
  }
}

function mapBackendItemUpdate(batchItem: BatchJobItem): Partial<BatchImportItem> | null {
  const itemStatus = (batchItem.status ?? '').toLowerCase();
  if (itemStatus === 'completed') {
    return { status: 'success', error: undefined };
  }
  if (itemStatus === 'failed') {
    return {
      status: 'error',
      error: batchItem.errorMessage ?? 'Import failed',
    };
  }
  if (itemStatus === 'cancelled' || itemStatus === 'canceled') {
    return { status: 'error', error: 'Import cancelled' };
  }
  if (itemStatus === 'processing' || itemStatus === 'running' || itemStatus === 'pending') {
    return { status: 'importing' };
  }
  return null;
}

function hydrateItemsFromBackend(backendItems: BatchJobItem[]): {
  items: BatchImportItem[];
  urlText: string;
} {
  const hydrated: BatchImportItem[] = [];
  const seen = new Set<string>();

  for (const [index, entry] of backendItems.entries()) {
    const target = entry.target ?? entry.url ?? '';
    if (!target || seen.has(target)) continue;
    seen.add(target);
    const mapped = mapBackendItemUpdate(entry);
    hydrated.push({
      id: entry.itemId || `reattached-${index}-${target}`,
      url: target,
      status: mapped?.status ?? 'importing',
      error: mapped?.error,
      selected: true,
    });
  }

  return {
    items: hydrated,
    urlText: hydrated.map((item) => item.url).join('\n'),
  };
}

let runCounter = 0;
let parseGeneration = 0;
let busy = false;
let cancelling = false;
let pollTimer: ReturnType<typeof setTimeout> | undefined;
let parseTimer: ReturnType<typeof setTimeout> | undefined;
let lastParsedText = '';
let parsePending = false;
let retryPollFn: (() => Promise<void>) | null = null;
let notifiedRunId: number | null = null;
const notifiedJobIds = new Set<string>();
let onImportCompleteHandler: ((results: BatchUrlImportResults) => void) | null = null;

function notifyCompletionOnce(
  run: number,
  jobId: string,
  results: BatchUrlImportResults,
  api: ApiClient,
): void {
  if (notifiedRunId === run || notifiedJobIds.has(jobId)) return;
  notifiedRunId = run;
  notifiedJobIds.add(jobId);

  if (onImportCompleteHandler) {
    onImportCompleteHandler(results);
    return;
  }

  if (results.failed > 0) {
    toast.error(`Couldn't import ${results.failed} URLs`);
    return;
  }

  void (async () => {
    const shape = await loadCorpusShapeSafe(api);
    const summary = composeIngestSummary('URL', results.successful, shape);
    toast.success(summary.title, {
      ...(summary.message ? { message: summary.message } : {}),
      action: { label: 'Open Library', onClick: openLibraryRoot },
    });
  })();
}

function startPollingJob(
  jobId: string,
  run: number,
  selectedSnapshot: BatchImportItem[],
  api: ApiClient,
  initialRunError: string | null = null,
): void {
  let lastProgressAt = Date.now();
  let lastProgressSignature: string | null = null;
  let consecutiveErrors = 0;
  let selected = selectedSnapshot;
  let itemIdsByTarget = new Map(selected.map((item) => [item.url, item.id]));
  useBatchUrlImportStore.setState({ runError: initialRunError });

  const updateItem = (itemId: string, updates: Partial<BatchImportItem>) => {
    useBatchUrlImportStore.setState((state) => ({
      items: state.items.map((item) =>
        item.id === itemId ? { ...item, ...updates } : item,
      ),
    }));
  };

  const pollJobStatus = async () => {
    if (runCounter !== run) return;
    try {
      const status = await getBatchJobStatus(jobId, api);
      if (runCounter !== run) return;

      consecutiveErrors = 0;

      if (selected.length === 0 && Array.isArray(status.items) && status.items.length > 0) {
        const currentItems = useBatchUrlImportStore.getState().items;
        if (currentItems.length === 0) {
          const hydrated = hydrateItemsFromBackend(status.items);
          selected = hydrated.items;
          itemIdsByTarget = new Map(selected.map((item) => [item.url, item.id]));
          const nextUrlText = useBatchUrlImportStore.getState().urlText || hydrated.urlText;
          lastParsedText = nextUrlText;
          parsePending = false;
          useBatchUrlImportStore.setState({
            items: hydrated.items,
            urlText: nextUrlText,
          });
        } else {
          selected = currentItems.filter((item) => item.selected);
          itemIdsByTarget = new Map(selected.map((item) => [item.url, item.id]));
        }
      }

      const normalizedStatus = status.status?.toLowerCase?.() ?? '';
      const completedCount = status.completedItems ?? status.completed_items ?? 0;
      const failedCount = status.failedItems ?? status.failed_items ?? 0;
      const backendTotal = status.totalItems ?? status.total_items;
      const totalCount = backendTotal ?? selected.length;
      const processedCount = completedCount + failedCount;
      const computedProgress = totalCount > 0
        ? Math.min(100, Math.round((processedCount / totalCount) * 100))
        : 0;

      const itemSignature = Array.isArray(status.items)
        ? status.items.map((entry) => `${entry.itemId}:${entry.status}`).join(',')
        : '';
      const progressSignature = `${normalizedStatus}|${completedCount}|${failedCount}|${totalCount}|${itemSignature}`;
      if (lastProgressSignature !== progressSignature) {
        lastProgressSignature = progressSignature;
        lastProgressAt = Date.now();
      }

      useBatchUrlImportStore.setState({
        runError: null,
        progress: Number.isFinite(status.progress) ? status.progress : computedProgress,
        importStats: {
          success: completedCount,
          error: failedCount,
          total: totalCount,
        },
      });

      if (Array.isArray(status.items) && status.items.length > 0) {
        status.items.forEach((batchItem) => {
          const targetId = itemIdsByTarget.get(batchItem.target ?? batchItem.url ?? '');
          if (!targetId) return;
          const mapped = mapBackendItemUpdate(batchItem);
          if (mapped) {
            updateItem(targetId, mapped);
          }
        });
      } else {
        selected.forEach((item, index) => {
          if (index < completedCount) {
            updateItem(item.id, { status: 'success', error: undefined });
          } else if (index < processedCount) {
            updateItem(item.id, { status: 'error', error: 'Import failed' });
          } else if (normalizedStatus === 'running' || normalizedStatus === 'pending') {
            updateItem(item.id, { status: 'importing' });
          }
        });
      }

      const reachedTerminalStatus = ['completed', 'failed', 'cancelled', 'canceled', 'interrupted'].includes(
        normalizedStatus,
      );
      const reachedExpectedCount = totalCount > 0 && processedCount >= totalCount;
      const timedOut = Date.now() - lastProgressAt >= BATCH_POLL_TIMEOUT_MS;

      if (timedOut && !reachedTerminalStatus && !reachedExpectedCount) {
        console.warn('[BatchImport] Polling timed out', { jobId, status });
        busy = false;
        useBatchUrlImportStore.setState((state) => ({
          isImporting: false,
          runError: 'Import status timed out',
          items: state.items.map((item) =>
            itemIdsByTarget.has(item.url) && item.status !== 'success'
              ? { ...item, status: 'error', error: 'Import status timed out' }
              : item,
          ),
        }));
        return;
      }

      if (reachedTerminalStatus || reachedExpectedCount) {
        busy = false;
        retryPollFn = null;
        const results: BatchUrlImportResults = {
          successful: completedCount,
          failed: failedCount,
        };
        useBatchUrlImportStore.setState({
          activeJobId: null,
          progress: 100,
          isImporting: false,
          runError: null,
        });
        notifyCompletionOnce(run, jobId, results, api);
        return;
      }

      clearTimeout(pollTimer);
      pollTimer = setTimeout(() => {
        void pollJobStatus();
      }, BATCH_POLL_INTERVAL_MS);
    } catch (error) {
      if (runCounter !== run) return;
      console.error('[BatchImport] Polling error:', error);
      consecutiveErrors += 1;
      const message = formatInvokeError(error);
      if (consecutiveErrors < MAX_CONSECUTIVE_POLL_ERRORS) {
        useBatchUrlImportStore.setState({
          isImporting: true,
          runError: message,
        });
        clearTimeout(pollTimer);
        pollTimer = setTimeout(() => {
          void pollJobStatus();
        }, BATCH_POLL_INTERVAL_MS);
        return;
      }
      busy = false;
      useBatchUrlImportStore.setState({
        isImporting: false,
        runError: message,
      });
    }
  };

  retryPollFn = async () => {
    clearTimeout(pollTimer);
    pollTimer = undefined;
    lastProgressAt = Date.now();
    consecutiveErrors = 0;
    busy = true;
    useBatchUrlImportStore.setState({
      runError: null,
      isImporting: true,
    });
    await pollJobStatus();
  };

  void pollJobStatus();
}

async function fetchItemPreview(
  itemId: string,
  url: string,
  generation: number,
  api: ApiClient,
): Promise<void> {
  useBatchUrlImportStore.setState((state) => {
    if (generation !== parseGeneration) return state;
    return {
      items: state.items.map((item) =>
        item.id === itemId ? { ...item, status: 'fetching' } : item,
      ),
    };
  });

  try {
    const result = await api.fetchUrlPreview(url);
    if (generation !== parseGeneration) return;
    if (!result.ok) {
      throw new Error(result.error);
    }
    const preview = result.data;
    useBatchUrlImportStore.setState((state) => {
      if (generation !== parseGeneration) return state;
      return {
        items: state.items.map((item) =>
          item.id === itemId ? { ...item, preview, status: 'ready' } : item,
        ),
      };
    });
  } catch (err) {
    if (generation !== parseGeneration) return;
    console.error('Preview failed:', err);
    const errorMessage = formatInvokeError(err);
    useBatchUrlImportStore.setState((state) => {
      if (generation !== parseGeneration) return state;
      return {
        items: state.items.map((item) =>
          item.id === itemId
            ? {
                ...item,
                status: 'error',
                error: errorMessage,
              }
            : item,
        ),
      };
    });
  }
}

function parseUrlsIntoStore(text: string, api: ApiClient): void {
  const generation = ++parseGeneration;
  lastParsedText = text;
  parsePending = false;
  const urls = extractUrls(text);
  const newItems: BatchImportItem[] = urls.map((url, index) => ({
    id: `${Date.now()}-${index}-${Math.random()}`,
    url,
    status: 'idle',
    selected: true,
  }));

  useBatchUrlImportStore.setState({ items: newItems });

  newItems.forEach((item) => {
    void fetchItemPreview(item.id, item.url, generation, api);
  });
}

async function findActiveBackendUrlJob(api: ApiClient): Promise<string | null> {
  if (typeof api.listBatchJobs !== 'function') return null;
  try {
    const jobs = await listAllBatchJobs(api);
    const active = jobs.find(
      (job) =>
        job.jobType === 'url_import' &&
        ACTIVE_JOB_STATUSES.has(job.status.toLowerCase()),
    );
    return active ? active.jobId || active.id : null;
  } catch {
    return null;
  }
}

export function setBatchUrlText(text: string, api: ApiClient): void {
  parsePending = true;
  useBatchUrlImportStore.getState().setUrlTextOnly(text);
  clearTimeout(parseTimer);
  parseTimer = setTimeout(() => {
    parseUrlsIntoStore(text, api);
  }, 500);
}

export function cancelPendingBatchUrlParse(): void {
  clearTimeout(parseTimer);
  parseTimer = undefined;
}

export function flushPendingBatchUrlParse(api: ApiClient): void {
  const state = useBatchUrlImportStore.getState();
  if (busy || state.isImporting || state.activeJobId !== null) return;
  if (!parsePending && state.urlText === lastParsedText) return;
  clearTimeout(parseTimer);
  parseTimer = undefined;
  parseUrlsIntoStore(state.urlText, api);
}

export function registerBatchUrlCompleteHandler(
  handler: ((results: BatchUrlImportResults) => void) | undefined,
): () => void {
  onImportCompleteHandler = handler ?? null;
  return () => {
    if (onImportCompleteHandler === (handler ?? null)) {
      onImportCompleteHandler = null;
    }
  };
}

export async function reattachActiveBatchUrlJob(api: ApiClient): Promise<void> {
  const state = useBatchUrlImportStore.getState();
  if (
    busy ||
    state.isImporting ||
    state.activeJobId !== null ||
    state.items.length > 0 ||
    state.urlText.trim().length > 0
  ) {
    return;
  }
  const jobId = await findActiveBackendUrlJob(api);
  if (!jobId || notifiedJobIds.has(jobId)) return;
  const latest = useBatchUrlImportStore.getState();
  if (
    busy ||
    latest.isImporting ||
    latest.activeJobId !== null ||
    latest.items.length > 0 ||
    latest.urlText.trim().length > 0
  ) {
    return;
  }

  busy = true;
  const run = ++runCounter;
  useBatchUrlImportStore.setState({
    isImporting: true,
    activeJobId: jobId,
    runError: null,
  });
  startPollingJob(jobId, run, [], api);
}

export async function startBatchUrlImportRun(api: ApiClient): Promise<void> {
  const initial = useBatchUrlImportStore.getState();
  if (busy || initial.isImporting || initial.activeJobId !== null) return;
  const selected = initial.items.filter(
    (item) => item.selected && (item.status === 'ready' || item.status === 'error'),
  );

  if (selected.length === 0) {
    return;
  }

  busy = true;
  const run = ++runCounter;

  useBatchUrlImportStore.setState({
    isImporting: true,
    runError: null,
    progress: 0,
    importStats: { success: 0, error: 0, total: selected.length },
  });

  try {
    const jobId = await startBatchUrlImport(
      selected.map((item) => item.url),
      { extractArticle: initial.options.extractArticle },
      api,
    );
    if (runCounter !== run) return;
    useBatchUrlImportStore.setState({ activeJobId: jobId });
    startPollingJob(jobId, run, selected, api);
  } catch (error) {
    if (runCounter !== run) return;
    console.error('[BatchImport] Start error:', error);
    busy = false;
    useBatchUrlImportStore.setState({
      isImporting: false,
      runError: formatInvokeError(error),
    });
  }
}

export async function retryBatchUrlPoll(): Promise<void> {
  await retryPollFn?.();
}

export async function cancelBatchUrlImport(api: ApiClient): Promise<void> {
  const jobId = useBatchUrlImportStore.getState().activeJobId;
  if (!jobId || cancelling) return;
  const run = runCounter;
  cancelling = true;
  useBatchUrlImportStore.setState({ isCancelling: true });
  try {
    await cancelBatchJob(jobId, api);
    if (runCounter !== run) return;
    runCounter += 1;
    clearTimeout(pollTimer);
    retryPollFn = null;
    busy = false;
    useBatchUrlImportStore.setState((state) => ({
      activeJobId: null,
      isImporting: false,
      runError: null,
      progress: 0,
      items: state.items.map((item) =>
        item.selected && item.status !== 'success'
          ? { ...item, status: 'error', error: 'Import cancelled' }
          : item,
      ),
    }));
  } catch (error) {
    if (runCounter === run) {
      useBatchUrlImportStore.setState({ runError: formatInvokeError(error) });
    }
  } finally {
    cancelling = false;
    useBatchUrlImportStore.setState({ isCancelling: false });
  }
}

export function resetBatchUrlImport(): void {
  runCounter += 1;
  parseGeneration += 1;
  busy = false;
  cancelling = false;
  clearTimeout(pollTimer);
  clearTimeout(parseTimer);
  pollTimer = undefined;
  parseTimer = undefined;
  lastParsedText = '';
  parsePending = false;
  retryPollFn = null;
  notifiedRunId = null;
  notifiedJobIds.clear();
  onImportCompleteHandler = null;
  useBatchUrlImportStore.getState().resetState();
}
