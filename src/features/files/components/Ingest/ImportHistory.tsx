/** Import results and recovery, backed by durable per-file job records. */
import { type FC, useState, useEffect, useCallback, useRef } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';

import { BATCH_IMPORT_JOBS } from '@/features/batch/jobKinds';
import { useJobStatus } from '@/features/jobs/api';
import type { BatchJobSummary, BatchJobStatus, BatchJobItem } from '@/types/api/batch';
import { listAllBatchJobs, deleteBatchJob, retryFailedItems, getBatchJobDetails } from '@/utils/batchHistory';
import { validateIndexablePath } from '@/utils/indexingFileValidation';

interface ImportHistoryProps { onRefresh?: () => void; }
interface ExpandedJob { open: boolean; details?: BatchJobStatus; loading: boolean; error?: string; }
const isActive = (job: BatchJobSummary) => ['running', 'pending', 'processing'].includes(job.status);
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const itemPriority = (status: string) => ['processing', 'running'].includes(status) ? 0 : status === 'failed' ? 1 : status === 'pending' ? 2 : 3;
const fileName = (path: string) => path.split(/[\\/]/).pop() || path;
/** Every import's summary, read together with the file details its rows need. */
const importHistoryKey = ['import-history'] as const;
/**
 * The import list and why the newest read, or a clear, failed. A failure keeps
 * the last list on screen beside its reason until a read succeeds.
 */
interface HistoryRead { jobs: BatchJobSummary[]; error?: string; }
/** While a read has failed, everything is read again on this cadence. */
const FAILED_READ_POLL_MS = 2000;
const NO_JOBS: BatchJobSummary[] = [];

function jobStatusLabel(job: BatchJobSummary): string {
  if (isActive(job) && job.completedItems + job.failedItems >= job.totalItems) return 'Finishing import…';
  if (isActive(job)) return `${job.completedItems} of ${job.totalItems} imported · ${Math.max(0, job.totalItems - job.completedItems - job.failedItems)} remaining`;
  if (job.failedItems > 0) return `${job.completedItems} imported, ${job.failedItems} failed`;
  if (job.status === 'cancelled') return 'Cancelled';
  if (job.status === 'failed') return 'Failed';
  return `${job.completedItems} imported`;
}

function recoveryHint(error: string | null): string {
  if (/OCR|selectable text/i.test(error ?? '')) return 'Choose a PDF with selectable text, or run OCR on it first.';
  if (/not found|no such file|does not exist/i.test(error ?? '')) return 'Choose the file again if it has moved or been renamed.';
  if (/timed? out|timeout/i.test(error ?? '')) return 'Retry the file. If it times out again, choose a smaller or re-exported PDF.';
  return 'Retry this file, or choose a corrected copy to replace it.';
}

export const ImportHistory: FC<ImportHistoryProps> = ({ onRefresh }) => {
  const queryClient = useQueryClient();
  const [expanded, setExpanded] = useState<Map<string, ExpandedJob>>(new Map());
  const expandedRef = useRef(expanded);
  expandedRef.current = expanded;
  const seen = useRef(new Set<string>());
  const busyRef = useRef(new Set<string>());
  const [busy, setBusy] = useState(new Set<string>());
  const [actionErrors, setActionErrors] = useState<Record<string, string>>({});
  const generation = useRef(0);
  // Whether the next read also refreshes every file list already read.
  const forceRef = useRef(false);
  const detailRequests = useRef(new Map<string, number>());

  const loadDetails = useCallback(async (id: string) => {
    const request = (detailRequests.current.get(id) ?? 0) + 1;
    detailRequests.current.set(id, request);
    const epoch = generation.current;
    const isCurrent = () => epoch === generation.current && detailRequests.current.get(id) === request;
    try {
      const details = await getBatchJobDetails(id);
      if (!isCurrent()) return;
      setExpanded(prev => new Map(prev).set(id, { open: prev.get(id)?.open ?? false, details, loading: false }));
    } catch (error) {
      if (!isCurrent()) return;
      setExpanded(prev => new Map(prev).set(id, { open: prev.get(id)?.open ?? false, details: prev.get(id)?.details, loading: false, error: message(error) }));
    }
  }, []);

  const loadJobDetails = useCallback(async (next: BatchJobSummary[], force: boolean) => {
    // Read single-file details even while collapsed, so their headings identify
    // the file. Keep every expanded/active result fresh through its final poll.
    const toLoad = next.filter(job => {
      const cached = expandedRef.current.get(job.id);
      const changed = cached?.details && (job.status !== cached.details.status
        || job.completedItems !== cached.details.completedItems || job.failedItems !== cached.details.failedItems);
      return (job.totalItems === 1 && !cached?.details) || cached?.open || cached?.error || isActive(job) || changed || (force && cached?.details)
        || (cached?.details && isActive(cached.details)) || (!seen.current.has(job.id) && job.failedItems > 0);
    });
    const autoOpen = new Set(next.filter(job => !seen.current.has(job.id) && (job.failedItems > 0 || isActive(job))).map(job => job.id));
    for (const job of next) seen.current.add(job.id);
    setExpanded(prev => {
      const updated = new Map(prev);
      for (const job of toLoad) if (!updated.has(job.id)) updated.set(job.id, { open: autoOpen.has(job.id), loading: true });
      return updated;
    });
    await Promise.all(toLoad.map(job => loadDetails(job.id)));
  }, [loadDetails]);

  const hasDetailErrors = Array.from(expanded.values()).some(detail => !!detail.error);
  const jobsQuery = useQuery({
    queryKey: importHistoryKey,
    // A list read superseded by a newer one, or by a retry, removal or clear, is
    // cancelled and discarded before it plans any file-detail reads.
    queryFn: async ({ signal }): Promise<HistoryRead> => {
      const force = forceRef.current;
      forceRef.current = false;
      const first = queryClient.getQueryState(importHistoryKey)?.status === 'pending';
      let next: BatchJobSummary[];
      try {
        next = await listAllBatchJobs();
      } catch (error) {
        return { jobs: queryClient.getQueryData<HistoryRead>(importHistoryKey)?.jobs ?? NO_JOBS, error: message(error) };
      }
      if (signal.aborted) return { jobs: next };
      const details = loadJobDetails(next, force);
      // Until the first read's file details arrive, the history shows as loading.
      if (first) await details;
      return { jobs: next };
    },
    // The file details live in this component's state, so the list must not outlive it.
    gcTime: 0,
    // A read that failed is tried again until it succeeds.
    refetchInterval: (query) => query.state.data?.error !== undefined || hasDetailErrors ? FAILED_READ_POLL_MS : false,
    refetchIntervalInBackground: true,
  });
  const jobs = jobsQuery.data?.jobs ?? NO_JOBS;
  const loading = jobsQuery.isPending;
  const loadError = jobsQuery.data?.error;
  const { refetch } = jobsQuery;
  /** A read that starts at once, superseding any in flight, and refreshes every file list read. */
  const refreshJobs = useCallback(() => {
    forceRef.current = true;
    void queryClient.cancelQueries({ queryKey: importHistoryKey });
    return refetch();
  }, [queryClient, refetch]);
  // A retry, removal or clear discards every list read in flight.
  const discardListReads = () => { void queryClient.cancelQueries({ queryKey: importHistoryKey }); };

  // An import's job reports each file it settles; the newest read wins.
  useJobStatus((report) => {
    if (BATCH_IMPORT_JOBS.has(report.kind)) void refreshJobs();
  });
  const hasActiveJobs = jobs.some(job => isActive(expanded.get(job.id)?.details ?? job));
  useEffect(() => {
    const refresh = () => { void refreshJobs(); };
    const visible = () => { if (document.visibilityState === 'visible') refresh(); };
    window.addEventListener('focus', refresh);
    document.addEventListener('visibilitychange', visible);
    return () => {
      window.removeEventListener('focus', refresh);
      document.removeEventListener('visibilitychange', visible);
    };
  }, [refreshJobs]);

  const toggle = (id: string) => {
    const opening = !expanded.get(id)?.open;
    setExpanded(prev => new Map(prev).set(id, { ...prev.get(id), open: opening, loading: opening && !prev.get(id)?.details }));
    if (opening) void loadDetails(id);
  };

  const recover = async (job: BatchJobSummary, item?: BatchJobItem, replace = false) => {
    if (busyRef.current.has(job.id) || isActive(job)) return;
    busyRef.current.add(job.id);
    setBusy(new Set(busyRef.current));
    setActionErrors(prev => ({ ...prev, [job.id]: '' }));
    try {
      let replacement: string | undefined;
      if (replace && item) {
        const chosen = await open({ multiple: false, directory: false, title: `Replace ${fileName(item.target)}`,
          ...(item.target.toLowerCase().endsWith('.pdf') ? { filters: [{ name: 'PDF', extensions: ['pdf'] }] } : {}),
        });
        if (!chosen) return; // Cancellation preserves the failed item and its reason.
        const validation = validateIndexablePath(chosen);
        if (!validation.ok) throw new Error('Choose a supported document file.');
        replacement = chosen;
      }
      // Discard reads started before this retry, even if they resolve later.
      generation.current += 1;
      discardListReads();
      const id = await retryFailedItems(job.id, item?.itemId, replacement);
      generation.current += 1;
      discardListReads();
      queryClient.setQueryData<HistoryRead>(importHistoryKey, prev => prev && { ...prev, jobs: prev.jobs.map(entry => entry.id === id ? { ...entry, status: 'pending' } : entry) });
      setExpanded(prev => new Map(prev).set(id, { open: true, loading: true }));
      await loadDetails(id);
      await refreshJobs();
      onRefresh?.();
    } catch (error) {
      setActionErrors(prev => ({ ...prev, [job.id]: `Could not start the retry: ${message(error)}` }));
    } finally {
      busyRef.current.delete(job.id);
      setBusy(new Set(busyRef.current));
    }
  };

  const remove = async (job: BatchJobSummary) => {
    try {
      await deleteBatchJob(job.id);
      generation.current += 1;
      discardListReads();
      queryClient.setQueryData<HistoryRead>(importHistoryKey, prev => prev && { ...prev, jobs: prev.jobs.filter(entry => entry.id !== job.id) });
      setExpanded(prev => { const next = new Map(prev); next.delete(job.id); return next; });
      onRefresh?.();
    } catch (error) { setActionErrors(prev => ({ ...prev, [job.id]: `Could not remove this import: ${message(error)}` })); }
  };

  const clearHistory = async () => {
    try {
      await Promise.all(jobs.map(job => deleteBatchJob(job.id)));
      generation.current += 1;
      discardListReads();
      queryClient.setQueryData<HistoryRead>(importHistoryKey, prev => prev && { ...prev, jobs: NO_JOBS });
      setExpanded(new Map());
      onRefresh?.();
    } catch (error) {
      queryClient.setQueryData<HistoryRead>(importHistoryKey, prev => ({ jobs: prev?.jobs ?? NO_JOBS, error: `Could not clear history: ${message(error)}` }));
    }
  };

  if (loading) return <p className="text-sm text-text-muted">Loading import history…</p>;
  return (
    <div>
      {loadError && <div role="alert" className="mb-4 text-sm text-text-secondary">Import status is unavailable: {loadError}{' '}
        <button type="button" onClick={() => { void refetch({ cancelRefetch: false }); }} className="text-accent underline">Refresh status</button>
      </div>}
      {!loadError && jobs.length === 0 && <p className="text-sm text-text-muted">No imports yet.</p>}
      {jobs.length > 0 && <>
        <div className="flex items-center justify-between pb-2">
          <h2 className="text-base font-medium text-text-primary">{jobs.length} {jobs.length === 1 ? 'import' : 'imports'}</h2>
          <button type="button" disabled={hasActiveJobs || busy.size > 0} onClick={() => { void clearHistory(); }} className="text-xs text-text-tertiary hover:text-text-primary disabled:opacity-50">Clear all</button>
        </div>
        <p className="pb-3 text-xs text-text-muted">Retrying a file updates its original import.</p>
        <div className="border-t border-border-subtle">
          {jobs.map(summary => {
            const detail = expanded.get(summary.id);
            // The heading and file rows must use the same status response. A
            // separately fetched list summary can be older or newer than it.
            const job = { ...summary, ...detail?.details };
            const singleTarget = job.totalItems === 1 ? detail?.details?.items[0]?.target : undefined;
            const title = singleTarget ? (job.jobType === 'file_import' ? fileName(singleTarget) : singleTarget) : `${job.totalItems} ${job.jobType === 'file_import' ? (job.totalItems === 1 ? 'file' : 'files') : (job.totalItems === 1 ? 'URL' : 'URLs')}`;
            const disabled = busy.has(job.id) || isActive(job);
            const items = [...(detail?.details?.items ?? [])].sort((a, b) => itemPriority(a.status) - itemPriority(b.status));
            return <section key={job.id} aria-label={`Import ${job.id}`} className="border-b border-border-subtle">
              <div className="flex flex-wrap items-center gap-3 py-3">
                <button type="button" onClick={() => toggle(job.id)} aria-expanded={detail?.open ?? false} className="min-w-0 flex-1 text-left">
                  <div className="wrap-break-word text-sm text-text-primary">{title}</div>
                  <div className="text-xs text-text-muted">Added {new Date(job.createdAt).toLocaleString()}</div>
                </button>
                <span role="status" className="text-xs text-text-secondary">{detail?.error ? 'Status unavailable' : jobStatusLabel(job)}</span>
                {job.failedItems > 0 && <button type="button" disabled={disabled} onClick={() => { void recover(job); }} className="text-xs text-accent hover:underline disabled:opacity-50">Retry all failed</button>}
                {!isActive(job) && <button type="button" disabled={disabled} onClick={() => { void remove(job); }} className="text-xs text-text-tertiary hover:text-text-primary">Remove</button>}
              </div>
              {actionErrors[job.id] && <p role="alert" className="pb-3 text-sm text-text-secondary">{actionErrors[job.id]}</p>}
              {detail?.error && <p role="alert" className="pb-3 text-sm text-text-secondary">Could not load files: {detail.error}. Displayed results may be out of date.{' '}
                <button type="button" onClick={() => { void loadDetails(job.id); }} className="text-accent underline">Refresh files</button>
              </p>}
              {detail?.open && <div className="pb-3 pl-4">
                {detail.loading && <p className="text-xs text-text-muted">Loading files…</p>}
                {items.map(item => <div key={item.itemId} className="border-t border-border-subtle py-3">
                  <p className="wrap-break-word text-sm text-text-primary">{job.jobType === 'file_import' ? fileName(item.target) : item.target}</p>
                  {job.jobType === 'file_import' && <p className="break-all text-xs text-text-muted">{item.target}</p>}
                  <p className="mt-1 whitespace-pre-wrap wrap-break-word text-xs text-text-secondary">{item.status === 'completed' ? 'Imported' : item.status === 'failed' ? `Failed — ${item.errorMessage || 'Import failed'}` : ['running', 'processing'].includes(item.status) ? 'Processing — not ready to search yet' : item.status === 'cancelled' ? 'Cancelled' : 'Queued'}</p>
                  {item.status === 'failed' && job.jobType === 'file_import' && <>
                    <p className="mt-1 text-xs text-text-muted">{recoveryHint(item.errorMessage ?? null)}</p>
                    <div className="mt-2 flex gap-4">
                      <button type="button" aria-label={`Retry ${fileName(item.target)}`} disabled={disabled} onClick={() => { void recover(job, item); }} className="text-xs text-accent hover:underline disabled:opacity-50">Retry file</button>
                      <button type="button" aria-label={`Choose replacement for ${fileName(item.target)}`} disabled={disabled} onClick={() => { void recover(job, item, true); }} className="text-xs text-accent hover:underline disabled:opacity-50">Choose replacement…</button>
                    </div>
                  </>}
                </div>)}
              </div>}
            </section>;
          })}
        </div>
      </>}
    </div>
  );
};
