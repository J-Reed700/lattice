/**
 * BatchUrlImport
 *
 * Paste many URLs, import them in one job. Reading-column surface: a
 * textarea, one primary action, and hairline rows for per-URL state.
 *
 * All workflow state and background polling live in `useBatchUrlImportStore`
 * so switching tabs or routes preserves live progress, errors, and one-shot
 * completion toasts.
 */

import { type FC, useEffect, useMemo, useRef } from 'react';

import { X } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/Checkbox';
import { IconButton } from '@/components/ui/IconButton';
import { Textarea } from '@/components/ui/textarea';
import { useApi } from '@/contexts/ApiContext';
import {
  cancelBatchUrlImport,
  cancelPendingBatchUrlParse,
  flushPendingBatchUrlParse,
  reattachActiveBatchUrlJob,
  registerBatchUrlCompleteHandler,
  retryBatchUrlPoll,
  setBatchUrlText,
  startBatchUrlImportRun,
} from '@/features/files/hooks/batchUrlImportRunner';
import {
  type BatchImportItem,
  useBatchUrlImportStore,
} from '@/features/files/stores/batchUrlImportStore';
import type { UrlPreview } from '@/types/api/web';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

interface BatchUrlImportProps {
  onImport?: (urls: string[]) => void;
  onImportComplete?: (results: { successful: number; failed: number }) => void;
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

export const BatchUrlImport: FC<BatchUrlImportProps> = ({ onImportComplete }) => {
  const api = useApi();
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const urlText = useBatchUrlImportStore((state) => state.urlText);
  const items = useBatchUrlImportStore((state) => state.items);
  const options = useBatchUrlImportStore((state) => state.options);
  const isImporting = useBatchUrlImportStore((state) => state.isImporting);
  const activeJobId = useBatchUrlImportStore((state) => state.activeJobId);
  const isCancelling = useBatchUrlImportStore((state) => state.isCancelling);
  const runError = useBatchUrlImportStore((state) => state.runError);
  const progress = useBatchUrlImportStore((state) => state.progress);
  const importStats = useBatchUrlImportStore((state) => state.importStats);

  const setExtractArticle = useBatchUrlImportStore((state) => state.setExtractArticle);
  const toggleItem = useBatchUrlImportStore((state) => state.toggleItem);
  const toggleAll = useBatchUrlImportStore((state) => state.toggleAll);
  const removeItem = useBatchUrlImportStore((state) => state.removeItem);

  useEffect(
    () => registerBatchUrlCompleteHandler(onImportComplete),
    [onImportComplete],
  );

  useEffect(() => {
    flushPendingBatchUrlParse(api);
    void reattachActiveBatchUrlJob(api);
  }, [api]);

  useEffect(() => () => cancelPendingBatchUrlParse(), []);

  const selectedItems = useMemo(
    () => items.filter((item) => item.selected),
    [items],
  );

  const hasActiveJob = isImporting || activeJobId !== null;
  const allSelected = items.length > 0 && items.every((item) => item.selected);
  const readyCount = selectedItems.filter(
    (item) => item.status === 'ready' || item.status === 'error',
  ).length;

  return (
    <div>
      <label htmlFor="url-textarea" className="sr-only">
        URLs to import, one per line
      </label>
      <Textarea
        ref={textareaRef}
        id="url-textarea"
        rows={7}
        value={urlText}
        disabled={hasActiveJob}
        onChange={(e) => setBatchUrlText(e.target.value, api)}
        placeholder="One URL per line"
      />

      <div className="mt-3.5 flex items-center justify-between gap-4">
        <Checkbox
          label="Extract article text"
          checked={options.extractArticle}
          disabled={hasActiveJob}
          onCheckedChange={(checked) => setExtractArticle(checked)}
        />

        <Button
          onClick={handleAsyncEvent(() => startBatchUrlImportRun(api))}
          disabled={readyCount === 0 || hasActiveJob}
          className="shrink-0"
        >
          {readyCount > 0
            ? `Import ${readyCount} ${readyCount === 1 ? 'URL' : 'URLs'}`
            : 'Import'}
        </Button>
      </div>

      {runError && (
        <div className="mt-4 text-sm text-danger-fg">
          <p role="alert">{runError}</p>
          {activeJobId && (
            <button
              type="button"
              disabled={isCancelling}
              onClick={() => void retryBatchUrlPoll()}
              className="mt-2 underline"
            >
              Retry status
            </button>
          )}
        </div>
      )}

      {hasActiveJob && (
        <div className="mt-4 flex items-center justify-between text-sm text-text-muted">
          <span className="tabular-nums">
            Importing… {importStats.success + importStats.error} of{' '}
            {importStats.total ?? selectedItems.length} · {Math.round(progress)}%
          </span>
          <button
            type="button"
            onClick={handleAsyncEvent(() => cancelBatchUrlImport(api))}
            disabled={!activeJobId || isCancelling}
            className="text-text-tertiary transition-colors duration-fast hover:text-text-primary"
          >
            {isCancelling ? 'Cancelling…' : 'Cancel'}
          </button>
        </div>
      )}

      {items.length > 0 && (
        <div className="mt-6">
          <div className="flex items-center justify-between pb-2">
            <h2 className="font-sans text-base font-semibold tracking-[-0.02em] text-text-primary">
              {items.length} {items.length === 1 ? 'URL' : 'URLs'}
            </h2>
            <button
              type="button"
              onClick={toggleAll}
              disabled={hasActiveJob}
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
                  <Checkbox
                    checked={item.selected}
                    disabled={hasActiveJob}
                    onCheckedChange={() => toggleItem(item.id)}
                    aria-label={`Include ${item.url}`}
                    className="mt-0.5 shrink-0"
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
                    disabled={hasActiveJob}
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
        <p className="mt-6 text-sm text-text-muted">No URLs yet.</p>
      )}
    </div>
  );
};
