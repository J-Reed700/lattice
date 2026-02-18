/**
 * IndexingProgress Component
 *
 * Displays real-time indexing progress with queue information.
 * Integrated with the progress tracking system.
 */

import { memo } from 'react';

import { Loader2 } from 'lucide-react';

import { useProgressStore } from '../../stores/progressStore';
import { ProgressBar } from '../Progress';

export interface IndexingProgressProps {
  /** Custom className */
  className?: string;
}

export const IndexingProgress = memo<IndexingProgressProps>(({ className = '' }) => {
  const operations = useProgressStore((state) => Array.from(state.operations.values()));

  // Get all indexing operations
  const indexingOperations = operations.filter((op) => op.type === 'indexing');
  const activeIndexing = indexingOperations.filter(
    (op) => op.status === 'running' || op.status === 'pending'
  );

  if (activeIndexing.length === 0) {
    return null;
  }

  // Show the most recent indexing operation
  const currentOperation = activeIndexing[0];

  return (
    <div className={`bg-[var(--accent-light)]/20 rounded-lg p-4 ${className}`}>
      <div className="flex items-start gap-3">
        <div className="flex-shrink-0">
          <Loader2 className="w-5 h-5 text-[var(--accent-primary)] animate-spin" />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center justify-between mb-2">
            <h4 className="text-sm font-semibold text-[var(--text-primary)]">
              Indexing in Progress
            </h4>
            <span className="text-xs text-[var(--text-secondary)]">
              {currentOperation.current} / {currentOperation.total}
            </span>
          </div>

          {currentOperation.message && (
            <p className="text-sm text-[var(--text-secondary)] mb-3 truncate">
              {currentOperation.message}
            </p>
          )}

          <ProgressBar
            progress={currentOperation.progress}
            variant="default"
            showLabel
            height={8}
          />

          {currentOperation.eta && currentOperation.eta > 0 && (
            <p className="text-xs text-[var(--text-secondary)] mt-2">
              Estimated time remaining:{' '}
              {currentOperation.eta < 60
                ? `${Math.round(currentOperation.eta)}s`
                : `${Math.floor(currentOperation.eta / 60)}m ${Math.round(currentOperation.eta % 60)}s`}
            </p>
          )}

          {activeIndexing.length > 1 && (
            <p className="text-xs text-[var(--text-secondary)] mt-2">
              +{activeIndexing.length - 1} more operations in queue
            </p>
          )}
        </div>
      </div>
    </div>
  );
});

IndexingProgress.displayName = 'IndexingProgress';
