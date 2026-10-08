import { useState } from 'react';

import { RotateCcw } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { FolderIndexStatus } from '@/stores/explorerStore';

import { count, formatEta, formatRate, indexPercent } from './indexProgress';

/** The pill's words for an index status. Exported for tests. */
export function describeIndexStatus(status: FolderIndexStatus): string {
  switch (status.state) {
    case 'scanning':
      return status.filesTotal > 0 ? `Scanning · ${count(status.filesTotal)} files` : 'Scanning files…';
    case 'indexing': {
      const percent = indexPercent(status.passagesEmbedded, status.passagesTotal);
      return status.etaSeconds === null
        ? `Indexing ${percent}%`
        : `Indexing ${percent}% · ${formatEta(status.etaSeconds)} left`;
    }
    case 'ready':
      return `Indexed · ${count(status.filesTotal)} files`;
    case 'tooLarge':
      return 'Too large to index · search uses text matching';
    case 'refused':
      return 'Not indexed (home folder)';
    case 'unavailable':
      return 'No embedding model';
    case 'error':
      return 'Index failed';
  }
}

/** "3,067 of 10,958 passages · 489 of 1,379 files", once there is anything to count. */
export function describeIndexCounts(status: FolderIndexStatus): string | null {
  if (status.passagesTotal === 0 && status.filesTotal === 0) return null;
  if (status.state === 'tooLarge' || status.state === 'refused') return null;
  return `${count(status.passagesEmbedded)} of ${count(status.passagesTotal)} passages · ${count(status.filesIndexed)} of ${count(status.filesTotal)} files`;
}

export interface IndexStatusPillProps {
  status: FolderIndexStatus;
  onRebuild: () => void;
  onRetry: () => void;
}

/**
 * Where the folder's search index stands, after the path in the scope bar.
 * While it builds, a thin bar under the words shows how many passages are in;
 * the pill opens a small panel with the counts, the rate and "Rebuild index".
 * A failed index offers Retry beside it.
 */
export function IndexStatusPill({ status, onRebuild, onRetry }: IndexStatusPillProps) {
  const [open, setOpen] = useState(false);
  const label = describeIndexStatus(status);
  const counts = describeIndexCounts(status);
  const busy = status.state === 'scanning' || status.state === 'indexing';
  const failed = status.state === 'error';
  const progress = indexPercent(status.passagesEmbedded, status.passagesTotal);

  return (
    <div className="flex shrink-0 items-center gap-1.5">
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label={`Folder index: ${label}`}
            data-state-index={status.state}
            className={`relative inline-flex h-6 max-w-68 items-center overflow-hidden rounded-full border px-2.5 text-[11.5px] tabular-nums transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.05)] ${
              failed ? 'border-[hsl(var(--danger)/0.4)] text-[hsl(var(--danger))]' : 'border-border-subtle text-text-secondary'
            }`}
          >
            <span className="truncate">{label}</span>
            {status.state === 'indexing' && (
              <span aria-hidden="true" className="absolute inset-x-0 bottom-0 h-[2px] bg-[hsl(var(--text-primary)/0.06)]">
                <span className="block h-full bg-accent transition-[width] duration-300" style={{ width: `${progress}%` }} />
              </span>
            )}
            {status.state === 'scanning' && (
              <span aria-hidden="true" className="absolute inset-x-0 bottom-0 h-[2px] animate-pulse bg-accent/40" />
            )}
          </button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-80 p-2">
          <div className="px-2 py-1.5">
            <p className="text-[12.5px] text-text-primary">{label}</p>
            {counts && <p className="mt-1 text-[12px] tabular-nums leading-5 text-text-secondary">{counts}</p>}
            {status.state === 'indexing' && status.passagesPerSecond !== null && (
              <p className="text-[12px] tabular-nums leading-5 text-text-secondary">{formatRate(status.passagesPerSecond)}</p>
            )}
            {status.message && <p className="mt-1 text-[12px] leading-5 text-text-secondary">{status.message}</p>}
            {status.indexRoot !== status.root && (
              <p className="mt-1 text-[12px] leading-5 text-text-secondary">Uses the index of {status.indexRoot}.</p>
            )}
            <p className="mt-1 text-[11.5px] leading-5 text-text-muted">
              {busy ? 'Search covers what is indexed so far. ' : ''}The index lives in Lattice&apos;s data, not in the folder.
            </p>
          </div>
          <button
            type="button"
            disabled={status.state === 'refused'}
            onClick={() => {
              setOpen(false);
              onRebuild();
            }}
            className="mt-1 flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px] text-text-primary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.05)] disabled:opacity-40"
          >
            <RotateCcw className="h-3.5 w-3.5 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
            Rebuild index
          </button>
        </PopoverContent>
      </Popover>
      {failed && (
        <button
          type="button"
          onClick={onRetry}
          className="h-6 rounded-md px-1.5 text-[12px] font-medium text-text-primary underline-offset-2 transition-colors duration-fast hover:underline"
        >
          Retry
        </button>
      )}
    </div>
  );
}
