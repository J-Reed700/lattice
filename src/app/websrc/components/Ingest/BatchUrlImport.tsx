/**
 * BatchUrlImport
 *
 * Purpose: Import multiple URLs at once with preview and batch processing
 *
 * Features:
 * - Multi-line textarea for bulk URL pasting
 * - Automatic URL extraction and validation
 * - Preview metadata fetching with loading states
 * - Selectable table with bulk actions
 * - Progress tracking during batch import
 * - Error handling per URL
 * - Modern animated UI with Framer Motion
 *
 * Visual States:
 * - Input: Paste URLs (one per line)
 * - Preview: Table with metadata and status
 * - Importing: Progress modal with per-URL status
 * - Complete: Summary with success/error counts
 *
 * Accessibility: WCAG AA, keyboard navigation, ARIA labels
 */

import { useState, useCallback, useMemo, useRef } from 'react';

import { motion, AnimatePresence } from 'framer-motion';
import {
  Globe,
  Loader2,
  CheckCircle2,
  XCircle,
  Trash2,
  Download,
  FileText,
  Clock,
  AlertCircle,
  Link as LinkIcon,
  X,
} from 'lucide-react';

import { Button } from '@/components/ui/button';
import { VaultAPI } from '@/lib/api';
import { cn } from '@/lib/utils';
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
    return 'Failed to fetch preview';
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

export const BatchUrlImport: React.FC<BatchUrlImportProps> = ({ onImportComplete }) => {
  const [urlText, setUrlText] = useState('');
  const [items, setItems] = useState<BatchImportItem[]>([]);
  const [options, setOptions] = useState<BatchImportOptions>({
    extractArticle: true,
  });
  const [isImporting, setIsImporting] = useState(false);
  const [progress, setProgress] = useState(0);
  const [importStats, setImportStats] = useState({ success: 0, error: 0 });
  const [lastRunResult, setLastRunResult] = useState<{ successful: number; failed: number } | null>(null);
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

  const removeSelected = () => {
    setItems((prev) => prev.filter((item) => !item.selected));
  };

  const removeItem = (itemId: string) => {
    setItems((prev) => prev.filter((item) => item.id !== itemId));
  };

  const selectedItems = useMemo(
    () => items.filter((item) => item.selected),
    [items]
  );
  const readyItemCount = useMemo(
    () => items.filter((item) => item.status === 'ready').length,
    [items]
  );
  const previewErrorCount = useMemo(
    () => items.filter((item) => item.status === 'error').length,
    [items]
  );
  const fetchingCount = useMemo(
    () => items.filter((item) => item.status === 'fetching').length,
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
  const someSelected = items.some((item) => item.selected);
  const readyCount = selectedItems.filter(
    (item) => item.status === 'ready' || item.status === 'error'
  ).length;

  return (
    <div className="space-y-6">
      {/* URL Input Section */}
      <div className="space-y-2">
        <label
          htmlFor="url-textarea"
          className="block text-sm font-medium text-[hsl(var(--text-secondary))]"
        >
          Paste URLs (one per line)
        </label>
        <div className="relative">
          <textarea
            ref={textareaRef}
            id="url-textarea"
            value={urlText}
            onChange={(e) => handleUrlTextChange(e.target.value)}
            placeholder="https://example.com/article-1&#10;https://example.com/article-2&#10;https://example.com/article-3"
            className={cn(
              'w-full h-32 px-4 py-3 resize-y',
              'bg-[hsl(var(--surface-raised))]',
              'text-[hsl(var(--text-primary))]',
              'border rounded-lg',
              'border-[hsl(var(--border-subtle))]',
              'placeholder-[hsl(var(--text-tertiary))]',
              'focus:outline-none focus:ring-2 focus:ring-[hsl(var(--accent))] focus:ring-offset-0',
              'transition-colors duration-fast',
              'font-mono text-sm'
            )}
          />
          {items.length > 0 && (
            <div className="absolute top-2 right-2 px-2 py-1 rounded-md bg-[hsl(var(--accent))]/10 text-[hsl(var(--accent))] text-xs font-medium">
              {items.length} URL{items.length !== 1 ? 's' : ''} found
            </div>
          )}
        </div>
      </div>

      {/* Options */}
      <div className="flex items-center gap-3 p-3 rounded-lg bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))]">
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={options.extractArticle}
            onChange={(e) =>
              setOptions({ ...options, extractArticle: e.target.checked })
            }
            className="w-4 h-4 rounded border-[hsl(var(--border-subtle))] text-[hsl(var(--accent))] focus:ring-2 focus:ring-[hsl(var(--accent))] focus:ring-offset-0"
          />
          <span className="text-sm text-[hsl(var(--text-primary))]">
            Extract article content (reader mode)
          </span>
        </label>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
        <div className="rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 py-2">
          <p className="text-[11px] uppercase tracking-wide text-[hsl(var(--text-tertiary))]">Ready</p>
          <p className="text-sm font-semibold text-[hsl(var(--success-fg))]">{readyItemCount}</p>
        </div>
        <div className="rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 py-2">
          <p className="text-[11px] uppercase tracking-wide text-[hsl(var(--text-tertiary))]">Checking</p>
          <p className="text-sm font-semibold text-[hsl(var(--accent))]">{fetchingCount}</p>
        </div>
        <div className="rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 py-2">
          <p className="text-[11px] uppercase tracking-wide text-[hsl(var(--text-tertiary))]">Preview Errors</p>
          <p className="text-sm font-semibold text-[hsl(var(--danger-fg))]">{previewErrorCount}</p>
        </div>
      </div>

      {lastRunResult && (
        <div className="rounded-lg border border-[hsl(var(--success-muted))] bg-[hsl(var(--success-muted))] px-4 py-3">
          <p className="text-sm font-medium text-[hsl(var(--success-fg))]">Last import finished</p>
          <p className="text-xs text-[hsl(var(--success-fg))]/80 mt-1">
            {lastRunResult.successful} succeeded, {lastRunResult.failed} failed.
          </p>
        </div>
      )}

      {/* Preview Table */}
      {items.length > 0 && (
        <div className="space-y-3">
          {/* Bulk Actions Bar */}
          <div className="flex items-center justify-between p-3 rounded-lg bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))]">
            <div className="flex items-center gap-3">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={allSelected}
                  onChange={toggleAll}
                  className="w-4 h-4 rounded border-[hsl(var(--border-subtle))] text-[hsl(var(--accent))] focus:ring-2 focus:ring-[hsl(var(--accent))] focus:ring-offset-0"
                />
                <span className="text-sm text-[hsl(var(--text-secondary))]">
                  {allSelected ? 'Deselect all' : 'Select all'}
                </span>
              </label>
              {someSelected && (
                <motion.span
                  initial={{ opacity: 0, x: -10 }}
                  animate={{ opacity: 1, x: 0 }}
                  className="text-sm text-[hsl(var(--text-secondary))]"
                >
                  {selectedItems.length} selected
                </motion.span>
              )}
            </div>

            <div className="flex items-center gap-2">
              {someSelected && (
                <Button
                  onClick={removeSelected}
                  variant="ghost"
                  size="sm"
                >
                  <Trash2 className="w-4 h-4 mr-2" />
                  Remove selected
                </Button>
              )}
              <Button
                onClick={handleBatchImport}
                disabled={readyCount === 0 || isImporting}
                size="sm"
              >
                {isImporting ? (
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                ) : (
                  <Download className="w-4 h-4 mr-2" />
                )}
                Import {readyCount > 0 ? `${readyCount}` : ''} selected
              </Button>
            </div>
          </div>

          {/* Table */}
          <div className="border rounded-lg overflow-hidden border-[hsl(var(--border-subtle))]">
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead className="bg-[hsl(var(--surface-raised))] border-b border-[hsl(var(--border-subtle))]">
                  <tr>
                    <th className="w-12 px-4 py-3" />
                    <th className="px-4 py-3 text-left text-xs font-medium text-[hsl(var(--text-secondary))] uppercase tracking-wider">
                      URL / Title
                    </th>
                    <th className="px-4 py-3 text-left text-xs font-medium text-[hsl(var(--text-secondary))] uppercase tracking-wider">
                      Details
                    </th>
                    <th className="px-4 py-3 text-center text-xs font-medium text-[hsl(var(--text-secondary))] uppercase tracking-wider">
                      Status
                    </th>
                    <th className="w-16 px-4 py-3" />
                  </tr>
                </thead>
                <tbody className="bg-[var(--surface)] divide-y divide-[hsl(var(--border-subtle))]">
                  <AnimatePresence mode="popLayout">
                    {items.map((item, index) => (
                      <motion.tr
                        key={item.id}
                        initial={{ opacity: 0, y: -10 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, x: -20 }}
                        transition={{ delay: index * 0.05 }}
                        className={cn(
                          'transition-colors',
                          item.selected && 'bg-[hsl(var(--accent))]/5'
                        )}
                      >
                        {/* Checkbox */}
                        <td className="px-4 py-3">
                          <input
                            type="checkbox"
                            checked={item.selected}
                            onChange={() => toggleItem(item.id)}
                            className="w-4 h-4 rounded border-[hsl(var(--border-subtle))] text-[hsl(var(--accent))] focus:ring-2 focus:ring-[hsl(var(--accent))] focus:ring-offset-0"
                          />
                        </td>

                        {/* URL / Title */}
                        <td className="px-4 py-3">
                          <div className="space-y-1">
                            {item.preview ? (
                              <>
                                <div className="font-medium text-[hsl(var(--text-primary))] line-clamp-1">
                                  {item.preview.title}
                                </div>
                                <div className="flex items-center gap-1 text-xs text-[hsl(var(--text-secondary))]">
                                  <LinkIcon className="w-3 h-3" />
                                  <span className="truncate max-w-md">
                                    {item.url}
                                  </span>
                                </div>
                              </>
                            ) : (
                              <div className="flex items-center gap-1 text-sm text-[hsl(var(--text-primary))]">
                                <LinkIcon className="w-3 h-3" />
                                <span className="truncate max-w-md">
                                  {item.url}
                                </span>
                              </div>
                            )}
                          </div>
                        </td>

                        {/* Details */}
                        <td className="px-4 py-3">
                          {item.preview ? (
                            <div className="flex gap-3 text-xs text-[hsl(var(--text-secondary))]">
                              {item.preview.siteName && (
                                <span className="flex items-center gap-1">
                                  <Globe className="w-3 h-3" />
                                  {item.preview.siteName}
                                </span>
                              )}
                              {item.preview.wordCount > 0 && (
                                <span className="flex items-center gap-1">
                                  <FileText className="w-3 h-3" />
                                  {item.preview.wordCount.toLocaleString()} words
                                </span>
                              )}
                              {item.preview.readingTimeMinutes > 0 && (
                                <span className="flex items-center gap-1">
                                  <Clock className="w-3 h-3" />
                                  {item.preview.readingTimeMinutes} min
                                </span>
                              )}
                            </div>
                          ) : (
                            <span className="text-xs text-[hsl(var(--text-tertiary))]">
                              —
                            </span>
                          )}
                        </td>

                        {/* Status */}
                        <td className="px-4 py-3">
                          <div className="flex justify-center">
                            <StatusBadge status={item.status} error={item.error} />
                          </div>
                        </td>

                        {/* Actions */}
                        <td className="px-4 py-3">
                          <button
                            onClick={() => removeItem(item.id)}
                            className="text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--danger-fg))] transition-colors"
                            aria-label="Remove URL"
                          >
                            <Trash2 className="w-4 h-4" />
                          </button>
                        </td>
                      </motion.tr>
                    ))}
                  </AnimatePresence>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

      {/* Empty State */}
      {items.length === 0 && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          className="flex flex-col items-center justify-center py-12 text-center"
        >
          <div className="w-16 h-16 rounded-full bg-[hsl(var(--surface-raised))] flex items-center justify-center mb-4">
            <LinkIcon className="w-8 h-8 text-[hsl(var(--text-tertiary))]" />
          </div>
          <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
            Paste URLs above to get started
          </h3>
          <p className="text-sm text-[hsl(var(--text-secondary))] max-w-md">
            Copy multiple URLs from your bookmarks, reading list, or browser tabs and
            paste them in the textarea above. We'll automatically extract and preview them.
          </p>
        </motion.div>
      )}

      {/* Progress Modal */}
      <AnimatePresence>
        {isImporting && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 z-50 flex items-center justify-center bg-[hsl(var(--overlay))] backdrop-blur-sm"
            onClick={(e) => {
              if (e.target === e.currentTarget) {
                cancelImport();
              }
            }}
          >
            <motion.div
              initial={{ scale: 0.95, opacity: 0 }}
              animate={{ scale: 1, opacity: 1 }}
              exit={{ scale: 0.95, opacity: 0 }}
              className="bg-[var(--surface)] rounded-xl shadow-md border border-[hsl(var(--border-subtle))] w-full max-w-2xl mx-4 overflow-hidden"
            >
              {/* Header */}
              <div className="px-6 py-4 border-b border-[hsl(var(--border-subtle))] flex items-center justify-between">
                <div>
                  <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))]">
                    Importing URLs
                  </h3>
                  <p className="text-sm text-[hsl(var(--text-secondary))] mt-1">
                    {importStats.success + importStats.error} of {selectedItems.length} processed
                  </p>
                </div>
                <button
                  onClick={cancelImport}
                  className="text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors"
                  aria-label="Cancel import"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>

              {/* Progress Bar */}
              <div className="px-6 py-4">
                <div className="h-2 bg-[hsl(var(--surface-raised))] rounded-full overflow-hidden">
                  <motion.div
                    className="h-full bg-gradient-to-r from-[hsl(var(--accent))] to-[var(--accent-secondary)] rounded-full"
                    initial={{ width: 0 }}
                    animate={{ width: `${progress}%` }}
                    transition={{ duration: 0.3 }}
                  />
                </div>
                <div className="flex justify-between mt-2 text-xs text-[hsl(var(--text-secondary))]">
                  <span>{Math.round(progress)}% complete</span>
                  <span>
                    {importStats.success} success, {importStats.error} errors
                  </span>
                </div>
              </div>

              {/* Item List */}
              <div className="px-6 pb-6 max-h-96 overflow-y-auto">
                <div className="space-y-2">
                  {selectedItems.map((item) => (
                    <div
                      key={item.id}
                      className="flex items-center gap-3 p-3 rounded-lg bg-[hsl(var(--surface-raised))]"
                    >
                      <StatusIcon status={item.status} />
                      <div className="flex-1 min-w-0">
                        <div className="text-sm font-medium text-[hsl(var(--text-primary))] truncate">
                          {item.preview?.title || item.url}
                        </div>
                        {item.error && (
                          <div className="text-xs text-[hsl(var(--danger-fg))] mt-1">
                            {item.error}
                          </div>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              {/* Footer */}
              <div className="px-6 py-4 border-t border-[hsl(var(--border-subtle))] flex justify-end">
                <Button onClick={cancelImport} variant="outline" size="sm">
                  {progress === 100 ? 'Close' : 'Cancel'}
                </Button>
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
};

// Helper Components

const StatusBadge: React.FC<{ status: ItemStatus; error?: string }> = ({
  status,
  error,
}) => {
  const variants = {
    idle: {
      icon: Clock,
      text: 'Pending',
      className: 'text-[hsl(var(--text-tertiary))] bg-[hsl(var(--surface-raised))]',
    },
    fetching: {
      icon: Loader2,
      text: 'Loading',
      className: 'text-[hsl(var(--accent))] bg-[hsl(var(--accent))]/10 animate-pulse',
    },
    ready: {
      icon: CheckCircle2,
      text: 'Ready',
      className: 'text-[hsl(var(--success-fg))] bg-[hsl(var(--success-muted))]',
    },
    importing: {
      icon: Loader2,
      text: 'Importing',
      className: 'text-[hsl(var(--accent))] bg-[hsl(var(--accent))]/10',
      spin: true,
    },
    success: {
      icon: CheckCircle2,
      text: 'Success',
      className: 'text-[hsl(var(--success-fg))] bg-[hsl(var(--success-muted))]',
    },
    error: {
      icon: XCircle,
      text: error || 'Error',
      className: 'text-[hsl(var(--danger-fg))] bg-[hsl(var(--danger-fg))]/10',
    },
  };

  const variant = variants[status];
  const Icon = variant.icon;

  return (
    <div
      className={cn(
        'inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium',
        variant.className
      )}
    >
      <Icon className={cn('w-3 h-3', 'spin' in variant && variant.spin && 'animate-spin')} />
      <span>{variant.text}</span>
    </div>
  );
};

const StatusIcon: React.FC<{ status: ItemStatus }> = ({ status }) => {
  const variants = {
    idle: { icon: Clock, className: 'text-[hsl(var(--text-tertiary))]' },
    fetching: { icon: Loader2, className: 'text-[hsl(var(--accent))] animate-spin' },
    ready: { icon: CheckCircle2, className: 'text-[hsl(var(--success-fg))]' },
    importing: { icon: Loader2, className: 'text-[hsl(var(--accent))] animate-spin' },
    success: { icon: CheckCircle2, className: 'text-[hsl(var(--success-fg))]' },
    error: { icon: AlertCircle, className: 'text-[hsl(var(--danger-fg))]' },
  };

  const variant = variants[status];
  const Icon = variant.icon;

  return <Icon className={cn('w-5 h-5', variant.className)} />;
};
