import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { AnimatePresence, motion } from 'framer-motion';
import { AlertTriangle, ChevronDown, ChevronRight, Download, X } from 'lucide-react';

import {
  formatProgressBytes,
  formatSpeed,
  normalizePercentage,
  STATE_LABELS,
  STATE_TEXT_CLASS,
} from './downloadFormat';
import { DownloadProgressBar } from './DownloadProgressBar';
import { DownloadRow, type DownloadRowActions } from './DownloadRow';
import { useAggregatedDownloads } from '../../hooks/useAggregatedDownloads';
import { useDownloadActions } from '../../hooks/useDownloads';
import { useDownloadState } from '../../hooks/useDownloadState';
import { showErrorToast } from '../../utils/toast';

import type { AggregatedDownload, DownloadStatus } from '../../types/downloads';

/**
 * Maps each `DownloadStatus` object back to the store key it is filed under.
 *
 * Actions must be dispatched with the **store key**, not `download.id` — for a
 * multi-file model download the key is `<model_id>:<filename>` while `id` may be
 * the backend row id. `useAggregatedDownloads` passes the same object references
 * through, so identity lookup is exact.
 */
function useStoreKeyLookup(
  downloads: Map<string, DownloadStatus>
): (download: DownloadStatus) => string {
  const lookup = useMemo(() => {
    const byIdentity = new Map<DownloadStatus, string>();
    for (const [key, value] of downloads) byIdentity.set(value, key);
    return byIdentity;
  }, [downloads]);

  return useCallback(
    (download: DownloadStatus) => lookup.get(download) ?? download.id,
    [lookup]
  );
}

interface AggregatedGroupProps {
  group: AggregatedDownload;
  actions: DownloadRowActions;
  storeKeyFor: (download: DownloadStatus) => string;
}

/** A multi-file model download, collapsed to one summary row by default. */
function AggregatedGroup({ group, actions, storeKeyFor }: AggregatedGroupProps) {
  const [expanded, setExpanded] = useState(false);
  const completed = group.downloads.filter((d) => d.state === 'Completed').length;
  const percentage = normalizePercentage(
    group.percentage,
    group.bytes_downloaded,
    group.total_bytes
  );

  return (
    <div className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-3">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
        className="flex w-full items-start gap-2 text-left"
      >
        <span className="mt-0.5 shrink-0 text-[hsl(var(--text-muted))]">
          {expanded ? (
            <ChevronDown className="h-4 w-4" strokeWidth={1.75} />
          ) : (
            <ChevronRight className="h-4 w-4" strokeWidth={1.75} />
          )}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm text-[hsl(var(--text-primary))]">
            {group.model_name}
          </span>
          <span className="mt-0.5 flex flex-wrap items-center gap-x-2 text-xs text-[hsl(var(--text-muted))]">
            <span className={STATE_TEXT_CLASS[group.state]}>{STATE_LABELS[group.state]}</span>
            <span aria-hidden="true">·</span>
            <span>
              {completed}/{group.downloads.length} files
            </span>
            <span aria-hidden="true">·</span>
            <span>{formatProgressBytes(group.bytes_downloaded, group.total_bytes)}</span>
            {group.state === 'Downloading' && (
              <>
                <span aria-hidden="true">·</span>
                <span>{formatSpeed(group.average_speed)}</span>
              </>
            )}
          </span>
        </span>
      </button>

      {group.state !== 'Completed' && (
        <div className="mt-2">
          <DownloadProgressBar
            percentage={percentage}
            state={group.state}
            label={`${group.model_name} download progress`}
          />
        </div>
      )}

      {group.error_message && (
        <p className="mt-1.5 break-words text-xs text-[hsl(var(--danger,0_70%_50%))]">
          {group.error_message}
        </p>
      )}

      {expanded && (
        <div className="mt-2 divide-y divide-[hsl(var(--border-subtle))] border-t border-[hsl(var(--border-subtle))]">
          {group.downloads.map((file) => (
            <DownloadRow
              key={storeKeyFor(file)}
              storeKey={storeKeyFor(file)}
              download={file}
              actions={actions}
              nested
            />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Slide-over panel listing every download the backend knows about.
 *
 * Reads only from `useDownloadStore`; the single IPC subscription lives in
 * `useDownloadsListener`, mounted once at the App level. Mounting this component
 * more than once is therefore safe — it adds no listeners.
 */
export function DownloadsDrawer() {
  const {
    isDrawerOpen: isOpen,
    closeDrawer,
    downloadsMap: downloads,
    listenerError,
    setListenerError,
  } = useDownloadState();

  const {
    pauseDownload,
    resumeDownload,
    cancelDownload,
    retryDownload,
    removeDownload,
    clearCompletedDownloads,
  } = useDownloadActions();

  const actions = useMemo<DownloadRowActions>(
    () => ({ pauseDownload, resumeDownload, cancelDownload, retryDownload, removeDownload }),
    [pauseDownload, resumeDownload, cancelDownload, retryDownload, removeDownload]
  );

  const storeKeyFor = useStoreKeyLookup(downloads);
  const allDownloads = useMemo(() => Array.from(downloads.values()), [downloads]);
  const { aggregated, standalone } = useAggregatedDownloads(allDownloads);

  const hasCompleted = allDownloads.some((d) => d.state === 'Completed');
  const isEmpty = aggregated.length === 0 && standalone.length === 0;

  const panelRef = useRef<HTMLDivElement | null>(null);
  const previouslyFocused = useRef<HTMLElement | null>(null);

  // Close on Escape, and restore focus to whatever opened the drawer.
  useEffect(() => {
    if (!isOpen) return undefined;

    previouslyFocused.current = document.activeElement as HTMLElement | null;
    panelRef.current?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation();
        closeDrawer();
      }
    };
    document.addEventListener('keydown', onKeyDown);

    return () => {
      document.removeEventListener('keydown', onKeyDown);
      previouslyFocused.current?.focus?.();
    };
  }, [isOpen, closeDrawer]);

  const handleClearCompleted = useCallback(async () => {
    try {
      await clearCompletedDownloads();
    } catch (error) {
      showErrorToast('Could not clear completed downloads', error);
    }
  }, [clearCompletedDownloads]);

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            className="fixed inset-0 z-40 bg-[hsl(var(--overlay))]"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            onClick={closeDrawer}
            aria-hidden="true"
          />

          <motion.div
            ref={panelRef}
            tabIndex={-1}
            role="dialog"
            aria-modal="true"
            aria-label="Downloads"
            className="fixed right-0 top-0 z-50 flex h-full w-full max-w-sm flex-col border-l border-[hsl(var(--border-subtle))] bg-[hsl(var(--bg))] shadow-md outline-none"
            initial={{ x: '100%' }}
            animate={{ x: 0 }}
            exit={{ x: '100%' }}
            transition={{ type: 'tween', duration: 0.2, ease: 'easeOut' }}
          >
            <header className="flex items-center gap-2 border-b border-[hsl(var(--border-subtle))] px-4 py-3">
              <Download className="h-4 w-4 text-[hsl(var(--text-muted))]" strokeWidth={1.75} />
              <h2 className="flex-1 font-serif text-sm font-semibold text-[hsl(var(--text-primary))]">
                Downloads
              </h2>
              {hasCompleted && (
                <button
                  type="button"
                  onClick={() => void handleClearCompleted()}
                  className="rounded-md px-2 py-1 text-xs text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]"
                >
                  Clear completed
                </button>
              )}
              <button
                type="button"
                onClick={closeDrawer}
                aria-label="Close downloads"
                className="flex h-8 w-8 items-center justify-center rounded-md text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]"
              >
                <X className="h-4 w-4" strokeWidth={1.75} />
              </button>
            </header>

            {listenerError && (
              <div className="flex items-start gap-2 border-b border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] px-4 py-2">
                <AlertTriangle
                  className="mt-0.5 h-4 w-4 shrink-0 text-[hsl(var(--danger,0_70%_50%))]"
                  strokeWidth={1.75}
                />
                <p className="flex-1 text-xs text-[hsl(var(--text-muted))]">{listenerError}</p>
                <button
                  type="button"
                  onClick={() => setListenerError(null)}
                  aria-label="Dismiss download error"
                  className="text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-primary))]"
                >
                  <X className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </div>
            )}

            <div className="flex-1 space-y-2 overflow-y-auto p-3">
              {isEmpty ? (
                <div className="flex h-full flex-col items-center justify-center px-6 text-center">
                  <Download
                    className="h-6 w-6 text-[hsl(var(--text-muted))]"
                    strokeWidth={1.5}
                  />
                  <p className="mt-3 text-sm text-[hsl(var(--text-primary))]">No downloads</p>
                  <p className="mt-1 text-xs text-[hsl(var(--text-muted))]">
                    Models you download will appear here.
                  </p>
                </div>
              ) : (
                <>
                  {aggregated.map((group) => (
                    <AggregatedGroup
                      key={group.model_id}
                      group={group}
                      actions={actions}
                      storeKeyFor={storeKeyFor}
                    />
                  ))}
                  {standalone.map((download) => (
                    <DownloadRow
                      key={storeKeyFor(download)}
                      storeKey={storeKeyFor(download)}
                      download={download}
                      actions={actions}
                    />
                  ))}
                </>
              )}
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
