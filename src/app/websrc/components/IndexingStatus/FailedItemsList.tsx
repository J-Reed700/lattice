import { useState } from 'react';

import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';
import type { IndexingFailure } from '@/types/api/indexing';

import {
  useRemoveIndexedFileMutation,
  useRetryIndexedFileMutation,
} from '../../hooks/queries/useIndexingStatusQuery';


const TEXT_BUTTON_CLASS =
  'shrink-0 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50';

interface FailedItemRowProps {
  failure: IndexingFailure;
  onDone: (path: string) => void;
}

function FailedItemRow({ failure, onDone }: FailedItemRowProps) {
  const retry = useRetryIndexedFileMutation();
  const remove = useRemoveIndexedFileMutation();
  const busy = retry.isPending || remove.isPending;

  const handleRetry = () => {
    retry.mutate(failure.path, {
      onSuccess: () => {
        toast.success(`Reindexing ${failure.fileName}`);
        onDone(failure.path);
      },
      onError: (error) => {
        toast.error("Couldn't reindex that file", { message: error.message });
      },
    });
  };

  const handleRemove = () => {
    remove.mutate(failure.path, {
      onSuccess: () => {
        toast.success(`Removed ${failure.fileName} from the index`);
        onDone(failure.path);
      },
      onError: (error) => {
        toast.error("Couldn't remove that file", { message: error.message });
      },
    });
  };

  return (
    <div className="border-b border-border-subtle py-2 last:border-b-0">
      <div className="flex items-center justify-between gap-3">
        <span className="truncate text-sm text-text-primary" title={failure.path}>
          {failure.fileName}
        </span>
        {retry.isPending ? (
          <span className="shrink-0 text-xs text-text-muted">Retrying…</span>
        ) : (
          <span className="flex shrink-0 items-center gap-3">
            <button type="button" className={TEXT_BUTTON_CLASS} onClick={handleRetry} disabled={busy}>
              Retry
            </button>
            <button
              type="button"
              className={cn(TEXT_BUTTON_CLASS, 'hover:text-danger-fg')}
              onClick={handleRemove}
              disabled={busy}
            >
              Remove
            </button>
          </span>
        )}
      </div>
      <p className="mt-0.5 line-clamp-2 text-xs text-text-muted">{failure.reason}</p>
    </div>
  );
}

export interface FailedItemsListProps {
  failures: IndexingFailure[];
  /** Cap the rendered rows; the rest collapse behind one muted line. */
  limit?: number;
  /** Paths the user has dismissed locally. */
  dismissed?: ReadonlySet<string>;
  /** Called when a row resolves itself (retried or removed). */
  onDismiss?: (path: string) => void;
}

/**
 * The files that failed since this run started, with the two things a user can
 * actually do about them. Actions sit on the row rather than behind hover — a
 * 320px popover in a rail is not a place to hide affordances.
 */
export function FailedItemsList({ failures, limit = 5, dismissed, onDismiss }: FailedItemsListProps) {
  const [locallyDone, setLocallyDone] = useState<ReadonlySet<string>>(() => new Set<string>());

  const handleDone = (path: string) => {
    setLocallyDone((prev) => new Set(prev).add(path));
    onDismiss?.(path);
  };

  const visible = failures.filter((f) => !dismissed?.has(f.path) && !locallyDone.has(f.path));
  if (visible.length === 0) return null;

  const shown = visible.slice(0, limit);
  const hidden = visible.length - shown.length;

  return (
    <div>
      {shown.map((failure) => (
        <FailedItemRow key={failure.path} failure={failure} onDone={handleDone} />
      ))}
      {hidden > 0 ? <p className="pt-2 text-xs text-text-muted">and {hidden} more</p> : null}
    </div>
  );
}
