import { useState } from 'react';

import { RotateCcw } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { FolderIndexStatus } from '@/stores/explorerStore';

const count = (value: number) => value.toLocaleString('en-US');

/** The pill's words for an index status. Exported for tests. */
export function describeIndexStatus(status: FolderIndexStatus): string {
  switch (status.state) {
    case 'scanning':
      return 'Scanning files…';
    case 'indexing':
      return `Indexing ${count(status.filesIndexed)} / ${count(status.filesTotal)} files`;
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

export interface IndexStatusPillProps {
  status: FolderIndexStatus;
  onRebuild: () => void;
  onRetry: () => void;
}

/**
 * Where the folder's search index stands, after the path in the scope bar.
 * While it builds, a thin bar under the words shows how far; the pill opens a
 * small menu with "Rebuild index". A failed index offers Retry beside it.
 */
export function IndexStatusPill({ status, onRebuild, onRetry }: IndexStatusPillProps) {
  const [open, setOpen] = useState(false);
  const label = describeIndexStatus(status);
  const busy = status.state === 'scanning' || status.state === 'indexing';
  const failed = status.state === 'error';
  const progress = status.filesTotal > 0 ? Math.min(1, status.filesIndexed / status.filesTotal) : 0;

  return (
    <div className="flex shrink-0 items-center gap-1.5">
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label={`Folder index: ${label}`}
            data-state-index={status.state}
            className={`relative inline-flex h-6 max-w-[17rem] items-center overflow-hidden rounded-full border px-2.5 text-[11.5px] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.05)] ${
              failed ? 'border-[hsl(var(--danger)/0.4)] text-[hsl(var(--danger))]' : 'border-border-subtle text-text-secondary'
            }`}
          >
            <span className="truncate">{label}</span>
            {status.state === 'indexing' && (
              <span aria-hidden="true" className="absolute inset-x-0 bottom-0 h-[2px] bg-[hsl(var(--text-primary)/0.06)]">
                <span className="block h-full bg-accent transition-[width] duration-300" style={{ width: `${Math.round(progress * 100)}%` }} />
              </span>
            )}
            {status.state === 'scanning' && (
              <span aria-hidden="true" className="absolute inset-x-0 bottom-0 h-[2px] animate-pulse bg-accent/40" />
            )}
          </button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-72 p-2">
          <div className="px-2 py-1.5">
            <p className="text-[12.5px] text-text-primary">{label}</p>
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
