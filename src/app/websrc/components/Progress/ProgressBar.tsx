/**
 * ProgressBar Component
 *
 * Reusable progress bar with multiple variants and smooth animations.
 * Supports determinate and indeterminate states.
 */

import { memo } from 'react';

export type ProgressVariant = 'default' | 'success' | 'error' | 'indeterminate';

export interface ProgressBarProps {
  /** Progress value from 0-100 */
  progress: number;
  /** Visual variant */
  variant?: ProgressVariant;
  /** Show percentage label */
  showLabel?: boolean;
  /** Height in pixels */
  height?: number;
  /** Custom className */
  className?: string;
  /** Accessible label */
  'aria-label'?: string;
}

export const ProgressBar = memo<ProgressBarProps>(
  ({
    progress,
    variant = 'default',
    showLabel = false,
    height = 8,
    className = '',
    'aria-label': ariaLabel = 'Progress',
  }) => {
    // Clamp progress between 0-100
    const clampedProgress = Math.min(100, Math.max(0, progress));
    const isIndeterminate = variant === 'indeterminate';

    // Color variants
    const variantColors = {
      default: 'bg-[var(--accent-primary)]',
      success: 'bg-[var(--success)]',
      error: 'bg-[var(--error)]',
      indeterminate: 'bg-[var(--accent-primary)]',
    };

    const bgColor = variantColors[variant];

    return (
      <div className={`w-full ${className}`}>
        <div
          role="progressbar"
          aria-label={ariaLabel}
          aria-valuenow={isIndeterminate ? undefined : clampedProgress}
          aria-valuemin={0}
          aria-valuemax={100}
          className="relative w-full bg-[var(--bg-tertiary)] rounded-full overflow-hidden"
          style={{ height }}
        >
          {isIndeterminate ? (
            // Indeterminate animation
            <div
              className={`absolute inset-0 ${bgColor} animate-indeterminate`}
              style={{
                animation: 'indeterminate 1.5s infinite ease-in-out',
              }}
            />
          ) : (
            // Determinate progress
            <div
              className={`h-full ${bgColor} transition-all duration-300 ease-out rounded-full`}
              style={{ width: `${clampedProgress}%` }}
            />
          )}
        </div>

        {showLabel && !isIndeterminate && (
          <div className="mt-1 text-xs text-[var(--text-secondary)] text-right">
            {Math.round(clampedProgress)}%
          </div>
        )}
      </div>
    );
  }
);

ProgressBar.displayName = 'ProgressBar';

// Add indeterminate animation to global styles if not present
// This can be added to index.css or tailwind config
export const progressBarStyles = `
@keyframes indeterminate {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(400%);
  }
}

.animate-indeterminate {
  animation: indeterminate 1.5s infinite ease-in-out;
  width: 25%;
}
`;
