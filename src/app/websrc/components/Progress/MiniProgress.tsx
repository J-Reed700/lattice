/**
 * MiniProgress Component
 *
 * Compact progress indicator for inline display.
 * Shows number of active operations with visual feedback.
 */

import { memo } from 'react';

import { Loader2 } from 'lucide-react';

import { useProgressStore, selectActiveCount } from '../../stores/progressStore';

export interface MiniProgressProps {
  /** Click handler to expand full progress view */
  onClick?: () => void;
  /** Custom className */
  className?: string;
}

export const MiniProgress = memo<MiniProgressProps>(({ onClick, className = '' }) => {
  const activeCount = useProgressStore(selectActiveCount);

  if (activeCount === 0) {
    return null;
  }

  return (
    <button
      onClick={onClick}
      className={`
        inline-flex items-center gap-2 px-3 py-1.5
        bg-[var(--accent-light)]/20
        border border-[var(--accent-light)]
        rounded-full
        text-sm font-medium text-[var(--accent-primary)]
        hover:bg-[var(--accent-light)]
        transition-all duration-200
        cursor-pointer
        ${className}
      `}
      aria-label={`${activeCount} operation${activeCount > 1 ? 's' : ''} in progress`}
      title="Click to view progress details"
    >
      <Loader2 className="w-4 h-4 animate-spin" aria-hidden="true" />
      <span>
        {activeCount} {activeCount === 1 ? 'operation' : 'operations'}
      </span>
    </button>
  );
});

MiniProgress.displayName = 'MiniProgress';
