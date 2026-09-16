/**
 * BatchUrlImport
 *
 * Paste many URLs, import them in one job. Reading-column surface: a
 * textarea, one primary action, and hairline rows for per-URL state.
 * See `.design/UX-OVERHAUL-BRIEF.md` §2A.
 */

import { useState, useCallback, useMemo, useRef } from 'react';

import { X } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { IconButton } from '@/components/ui/IconButton';
import { settingsTextareaClass } from '@/components/ui/SettingsSection';
import { VaultAPI } from '@/lib/api';
import type { UrlPreview } from '@/types/api/web';
import { startBatchUrlImport, getBatchJobStatus } from '@/utils/batchImport';

type ItemStatus = 'idle' | 'fetching' | 'ready' | 'importing' | 'success' | 'error';

interface BatchImportItem {
  id: string;
  url: string;
  preview?: UrlPreview;
  status: ItemStatus;
  error?: string;
  selected: boolean;
}

interface BatchImportOptions {
  extractArticle: boolean;
}

interface BatchUrlImportProps {
  onImport?: (urls: string[]) => void;
  onImportComplete?: (results: { successful: number; failed: number }) => void;
}

const BATCH_POLL_INTERVAL_MS = 800;
const BATCH_POLL_TIMEOUT_MS = 180000;

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
    return `https://${trimmed}`;
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

function debounce<T extends (...args: never[]) => unknown>(
  func: T,
  delay: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout>;
  return (...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => func(...args), delay);
  };
}

/** Status as plain muted text. No badges, no colored dots. */
function statusLabel(item: BatchImportItem): string {
  switch (item.status) {
    case 'idle':
      return 'Pending';
    case 'fetching':
      return 'Checking…';
    case 'ready':
      return item.selected ? 'Ready' : 'Skipped';
    case 'importing':
      return 'Importing…';
    case 'success':
      return 'Imported';
    case 'error':
      return item.error ? `Failed — ${item.error}` : 'Failed';
    default:
      return '';
  }
}

function itemMeta(preview?: UrlPreview): string {
  if (!preview) return '';
  const parts: string[] = [];
  if (preview.siteName) parts.push(preview.siteName);
  if (preview.readingTimeMinutes > 0) parts.push(`${preview.readingTimeMinutes} min read`);
  else if (preview.wordCount > 0) parts.push(`${preview.wordCount.toLocaleString()} words`);
  return parts.join(' · ');
}

export const BatchUrlImport: React.FC<BatchUrlImportProps> = ({ onImportComplete }) => {
  const [urlText, setUrlText] = useState('');
  const [items, setItems] = useState<BatchImportItem[]>([]);
  const [options, setOptions] = useState<BatchImportOptions>({
    extractArticle: true,
  });
  const [isImporting, setIsImporting] = useState(false);
  const [progress, setProgress] = useState(0);
  const [importStats, setImportStats] = useState({ success: 0, error: 0 });
  // Kept as state, not rendered: the hub's toast is the one place a run's
  // outcome is stated, so the same fact is never on screen twice.
  const [, setLastRunResult] = useState<{ successful: number; failed: number } | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Parse URLs from textarea
  const handleUrlTextChange = (text: string) => {
    setUrlText(text);
    debouncedParseUrls(text);
  };

  const parseUrls = useCallback((text: string): void => {
    const urls = extractUrls(text);
    const newItems: BatchImportItem[] = urls.map((url) => ({
      id: `${Date.now()}-${Math.random()}`,
      url,
      status: 'idle',
      selected: true,
    }));

    setItems(newItems);

    // Fetch previews for all URLs
    newItems.forEach((item) => {
      fetchPreview(item.id, item.url);
    });
  }, []);

  const debouncedParseUrls = useMemo(
    () => debounce(parseUrls, 500),
    [parseUrls]
  );

  const fetchPreview = async (itemId: string, url: string) => {
    setItems((prev) =>
      prev.map((item) =>
        item.id === itemId ? { ...item, status: 'fetching' } : item
      )
    );

    try {
      const result = await VaultAPI.fetchUrlPreview(url);
      if (!result.ok) {
        throw new Error(result.error);
      }
      const preview = result.data;
      setItems((prev) =>
        prev.map((item) =>
          item.id === itemId
            ? { ...item, preview, status: 'ready' }
            : item
        )
      );
    } catch (err) {
      console.error('Preview failed:', err);
      const errorMessage = formatInvokeError(err);
      setItems((prev) =>
        prev.map((item) =>
          item.id === itemId
            ? {
                ...item,
                status: 'error',
                error: errorMessage,
              }
            : item
        )
      );
    }
  };

  const toggleItem = (itemId: string) => {
    setItems((prev) =>
      prev.map((item) =>
        item.id === itemId ? { ...item, selected: !item.selected } : item
      )
    );
  };

  const toggleAll = () => {
    const allSelected = items.every((item) => item.selected);
    setItems((prev) =>
      prev.map((item) => ({ ...item, selected: !allSelected }))
    );
  };

  const removeItem = (itemId: string) => {
    setItems((prev) => prev.filter((item) => item.id !== itemId));
  };

  const selectedItems = useMemo(
    () => items.filter((item) => item.selected),
    [items]
  );

  const updateItem = (itemId: string, updates: Partial<BatchImportItem>) => {
    setItems((prev) =>
      prev.map((item) => (item.id === itemId ? { ...item, ...updates } : item))
    );
  };

  const handleBatchImport = async () => {
    setIsImporting(true);
    setProgress(0);
    setImportStats({ success: 0, error: 0 });
    setLastRunResult(null);
    const selected = items.filter(
      (item) => item.selected && (item.status === 'ready' || item.status === 'error')
    );

    if (selected.length === 0) {
      setIsImporting(false);
      return;
    }

    try {
      const jobId = await startBatchUrlImport(
        selected.map(item => item.url),
        { extractArticle: options.extractArticle },
      );
      const startedAt = Date.now();
      const selectedCount = selected.length;
      const itemIdsByTarget = new Map(selected.map((item) => [item.url, item.id]));

      const pollJobStatus = async () => {
        try {
          const status = await getBatchJobStatus(jobId);
          const normalizedStatus = status.status?.toLowerCase?.() ?? '';
          const completedCount = status.completedItems ?? status.completed_items ?? 0;
          const failedCount = status.failedItems ?? status.failed_items ?? 0;
          const totalCount = status.totalItems ?? status.total_items ?? selectedCount;
          const processedCount = completedCount + failedCount;
          const computedProgress = totalCount > 0
            ? Math.min(100, Math.round((processedCount / totalCount) * 100))
            : 0;

          setProgress(Number.isFinite(status.progress) ? status.progress : computedProgress);
          setImportStats({
            success: completedCount,
            error: failedCount,
          });

          if (Array.isArray(status.items) && status.items.length > 0) {
            status.items.forEach((batchItem) => {
              const targetId = itemIdsByTarget.get(batchItem.target ?? '');
              if (!targetId) {
                return;
              }
              const itemStatus = (batchItem.status ?? '').toLowerCase();
              if (itemStatus === 'completed') {
                updateItem(targetId, { status: 'success', error: undefined });
              } else if (itemStatus === 'failed') {
                updateItem(targetId, {
                  status: 'error',
                  error: batchItem.errorMessage ?? 'Import failed',
                });
              } else if (itemStatus === 'processing' || itemStatus === 'running' || itemStatus === 'pending') {
                updateItem(targetId, { status: 'importing' });
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

          const reachedTerminalStatus = ['completed', 'failed', 'cancelled', 'canceled'].includes(normalizedStatus);
          const reachedExpectedCount = processedCount >= totalCount || processedCount >= selectedCount;
          const timedOut = Date.now() - startedAt > BATCH_POLL_TIMEOUT_MS;

          if (timedOut) {
            console.warn('[BatchImport] Polling timed out', { jobId, status });
            setIsImporting(false);
            selected.forEach((item) => {
              if (item.status === 'importing' || item.status === 'ready') {
                updateItem(item.id, { status: 'error', error: 'Import status timed out' });
              }
            });
            return;
          }

          if (reachedTerminalStatus || reachedExpectedCount) {
            setProgress(100);
            setIsImporting(false);
            setLastRunResult({
              successful: completedCount,
              failed: failedCount,
            });
            onImportComplete?.({
              successful: completedCount,
              failed: failedCount,
            });
            return;
          }

          setTimeout(pollJobStatus, BATCH_POLL_INTERVAL_MS);
        } catch (error) {
          console.error('[BatchImport] Polling error:', error);
          setIsImporting(false);
        }
      };

      void pollJobStatus();

    } catch (error) {
      console.error('[BatchImport] Start error:', error);
      setIsImporting(false);
    }
  };

  const cancelImport = () => {
    setIsImporting(false);
    setProgress(0);
  };

  const allSelected = items.length > 0 && items.every((item) => item.selected);
  const readyCount = selectedItems.filter(
    (item) => item.status === 'ready' || item.status === 'error'
  ).length;

  return (
    <div>
      <label htmlFor="url-textarea" className="sr-only">
        URLs to import, one per line
      </label>
      <textarea
        ref={textareaRef}
        id="url-textarea"
        rows={8}
        value={urlText}
        onChange={(e) => handleUrlTextChange(e.target.value)}
        placeholder="One URL per line"
        className={settingsTextareaClass}
      />

      <div className="mt-3 flex items-center justify-between gap-4">
        <label className="flex cursor-pointer items-center gap-2 text-sm text-text-secondary">
          <input
            type="checkbox"
            checked={options.extractArticle}
            onChange={(e) =>
              setOptions({ ...options, extractArticle: e.target.checked })
            }
            className="h-3.5 w-3.5 rounded-sm border-border-default accent-[hsl(var(--text-tertiary))]"
          />
          Extract article text
        </label>

        <Button
          onClick={handleBatchImport}
          disabled={readyCount === 0 || isImporting}
          className="h-9 shrink-0"
        >
          {readyCount > 0
            ? `Import ${readyCount} ${readyCount === 1 ? 'URL' : 'URLs'}`
            : 'Import'}
        </Button>
      </div>

      {isImporting && (
        <div className="mt-4 flex items-center justify-between text-sm text-text-muted">
          <span className="tabular-nums">
            Importing… {importStats.success + importStats.error} of {selectedItems.length} ·{' '}
            {Math.round(progress)}%
          </span>
          <button
            type="button"
            onClick={cancelImport}
            className="text-text-tertiary transition-colors duration-fast hover:text-text-primary"
          >
            Cancel
          </button>
        </div>
      )}

      {items.length > 0 && (
        <div className="mt-8">
          <div className="flex items-center justify-between pb-2">
            <h2 className="text-base font-medium text-text-primary">
              {items.length} {items.length === 1 ? 'URL' : 'URLs'}
            </h2>
            <button
              type="button"
              onClick={toggleAll}
              className="text-xs text-text-tertiary transition-colors duration-fast hover:text-text-primary"
            >
              {allSelected ? 'Deselect all' : 'Select all'}
            </button>
          </div>

          <div className="border-t border-border-subtle">
            {items.map((item) => {
              const meta = itemMeta(item.preview);
              return (
                <div
                  key={item.id}
                  className="group flex items-start gap-3 border-b border-border-subtle py-3"
                >
                  <input
                    type="checkbox"
                    checked={item.selected}
                    onChange={() => toggleItem(item.id)}
                    aria-label={`Include ${item.url}`}
                    className="mt-1 h-3.5 w-3.5 shrink-0 rounded-sm border-border-default accent-[hsl(var(--text-tertiary))]"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm text-text-primary">
                      {item.preview?.title || item.url}
                    </div>
                    <div className="truncate text-xs text-text-muted">
                      {item.preview?.title ? item.url : meta}
                    </div>
                    {item.preview?.title && meta && (
                      <div className="truncate text-xs text-text-muted">{meta}</div>
                    )}
                  </div>
                  <span className="max-w-[240px] shrink-0 truncate pt-0.5 text-xs text-text-muted">
                    {statusLabel(item)}
                  </span>
                  <IconButton
                    label="Remove URL"
                    onClick={() => removeItem(item.id)}
                    className="-mt-0.5 shrink-0 opacity-0 transition-opacity duration-fast focus-visible:opacity-100 group-hover:opacity-100"
                  >
                    <X />
                  </IconButton>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {items.length === 0 && (
        <p className="mt-8 text-sm text-text-muted">No URLs yet.</p>
      )}
    </div>
  );
};
