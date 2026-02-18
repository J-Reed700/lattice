/**
 * IndexProgress Component
 *
 * Purpose: Pure UI component for displaying real-time progress of document indexing operations
 *
 * This component follows the Hook Injection Pattern:
 * - All state management and event handling is delegated to useIndexProgress hook
 * - Component is purely presentational, only handling UI rendering
 * - Testing is simplified through hook mocking instead of complex event mocking
 *
 * Features:
 * - Live progress bar with percentage
 * - File counter (processed/total)
 * - Current file being indexed
 * - ETA estimation
 * - Pause/Resume/Cancel controls
 * - Multiple status states (idle, indexing, complete, error)
 *
 * States: idle, scanning, processing, complete, error, cancelled
 * Accessibility: WCAG AA, keyboard navigation, screen reader support
 */

import { CheckCircle2, XCircle, AlertCircle, Loader2, X } from 'lucide-react';

import { useIndexProgress } from '../../hooks/useIndexProgress';
import { handleAsyncEvent } from '../../utils/promiseHandlers';

interface IndexProgressProps {
  /** Show progress in compact mode (for header/sidebar) */
  compact?: boolean;
  /** Callback when indexing completes */
  onComplete?: () => void;
  /** Callback when indexing encounters an error */
  onError?: (error: string) => void;
  /** Callback when indexing is cancelled */
  onCancel?: () => void;
  /** Custom CSS classes */
  className?: string;
}

export function IndexProgress({
  compact = false,
  onComplete,
  onError,
  onCancel,
  className = '',
}: IndexProgressProps) {
  // Use the custom hook for all state management and event handling
  const { progress, error, isActive, handleCancel } = useIndexProgress({
    onComplete,
    onError,
    onCancel,
  });

  // Format time remaining
  const formatTime = (ms?: number): string => {
    if (!ms || ms <= 0) return 'Calculating...';
    const seconds = Math.floor(ms / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const secs = seconds % 60;
    if (minutes < 60) return `${minutes}m ${secs}s`;
    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    return `${hours}h ${mins}m`;
  };

  // Format file path for display (show only filename)
  const formatFileName = (path?: string): string => {
    if (!path) return '';
    const parts = path.split(/[\\/]/);
    return parts[parts.length - 1];
  };

  // Handle pause/resume
  // Note: Pause/resume functionality requires backend API implementation
  // The backend currently does not expose pause_indexing/resume_indexing commands
  // const handlePauseResume = () => {
  //   setIsPaused(!isPaused);
  //   // When backend implements pause/resume, call:
  //   // await invoke(isPaused ? 'resume_indexing' : 'pause_indexing')
  // };

  // Get status icon and color
  const getStatusDisplay = () => {
    if (error || progress?.status === 'error') {
      return {
        icon: <XCircle className="w-5 h-5" />,
        color: 'text-[var(--error)]',
        bgColor: 'bg-[var(--error-light)]/30',
        label: 'Error',
      };
    }

    switch (progress?.status) {
      case 'complete':
        return {
          icon: <CheckCircle2 className="w-5 h-5" />,
          color: 'text-[var(--success)]',
          bgColor: 'bg-[var(--success-light)]/30',
          label: 'Complete',
        };
      case 'cancelled':
        return {
          icon: <AlertCircle className="w-5 h-5" />,
          color: 'text-[var(--warning)]',
          bgColor: 'bg-[var(--warning-light)]/30',
          label: 'Cancelled',
        };
      case 'scanning':
        return {
          icon: <Loader2 className="w-5 h-5 animate-spin" />,
          color: 'text-[var(--accent-primary)]',
          bgColor: 'bg-[var(--accent-light)]/30',
          label: 'Scanning',
        };
      case 'processing':
        return {
          icon: <Loader2 className="w-5 h-5 animate-spin" />,
          color: 'text-[var(--accent-primary)]',
          bgColor: 'bg-[var(--accent-light)]/30',
          label: 'Processing',
        };
      default:
        return {
          icon: <Loader2 className="w-5 h-5" />,
          color: 'text-[var(--text-secondary)]',
          bgColor: 'bg-[var(--bg-primary)]',
          label: 'Idle',
        };
    }
  };

  // Don't render if no progress data
  if (!progress && !error) {
    return null;
  }

  const status = getStatusDisplay();
  const percentage = progress?.percentage ?? 0;

  // Compact mode (for header/sidebar)
  if (compact) {
    return (
      <div className={`flex items-center gap-2 ${className}`}>
        <div className={`${status.bgColor} rounded-full p-1.5`}>
          {status.icon}
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-xs font-medium text-[var(--text-secondary)] truncate">
            {status.label}
          </div>
          {progress && isActive && (
            <div className="text-xs text-[var(--text-tertiary)]">
              {progress.processed}/{progress.totalFiles}
            </div>
          )}
        </div>
        {isActive && (
          <button
            onClick={handleAsyncEvent(handleCancel)}
            className="p-1 hover:bg-[var(--bg-secondary)] rounded transition-colors"
            aria-label="Cancel indexing"
          >
            <X className="w-4 h-4 text-[var(--text-tertiary)]" />
          </button>
        )}
      </div>
    );
  }

  // Full mode
  return (
    <div className={`bg-[var(--surface-elevated)] rounded-lg border border-[var(--border-color)] shadow-sm overflow-hidden ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-[var(--border-color)]">
        <div className="flex items-center gap-3">
          <div className={`${status.bgColor} rounded-full p-2`}>
            {status.icon}
          </div>
          <div>
            <h3 className="text-sm font-semibold text-[var(--text-primary)]">
              {status.label}
            </h3>
            {isActive && progress && (
              <p className="text-xs text-[var(--text-secondary)]">
                Processing documents
              </p>
            )}
          </div>
        </div>

        {/* Action buttons */}
        {isActive && (
          <div className="flex items-center gap-2">
            {/* Pause/Resume button (placeholder for future implementation) */}
            {/* <button
              onClick={handlePauseResume}
              className="p-2 hover:bg-[var(--surface-hover)] rounded transition-colors"
              aria-label={isPaused ? 'Resume indexing' : 'Pause indexing'}
            >
              {isPaused ? (
                <Play className="w-4 h-4 text-[var(--text-secondary)]" />
              ) : (
                <Pause className="w-4 h-4 text-[var(--text-secondary)]" />
              )}
            </button> */}

            <button
              onClick={handleAsyncEvent(handleCancel)}
              className="px-3 py-1.5 text-xs font-medium text-[var(--error)] hover:bg-[var(--error-light)] rounded transition-colors"
              aria-label="Cancel indexing"
            >
              Cancel
            </button>
          </div>
        )}
      </div>

      {/* Progress content */}
      <div className="p-4">
        {error && (
          <div className="mb-4 p-3 bg-[var(--error-light)]/20 border border-[var(--error-light)] rounded">
            <p className="text-sm text-[var(--error)]">{error}</p>
          </div>
        )}

        {progress && (
          <>
            {/* Progress bar */}
            <div className="mb-4">
              <div className="flex items-center justify-between mb-2">
                <span className="text-sm font-medium text-[var(--text-secondary)]">
                  {progress.processed} / {progress.totalFiles} files
                </span>
                <span className="text-sm font-semibold text-[var(--accent-primary)]">
                  {percentage.toFixed(1)}%
                </span>
              </div>
              <div className="w-full bg-[var(--bg-tertiary)] rounded-full h-2 overflow-hidden">
                <div
                  className="bg-[var(--accent-primary)] h-2 rounded-full transition-all duration-300 ease-out"
                  style={{ width: `${percentage}%` }}
                  role="progressbar"
                  aria-valuenow={percentage}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-label="Indexing progress"
                />
              </div>
            </div>

            {/* Current file */}
            {isActive && progress.currentFile && (
              <div className="mb-3">
                <p className="text-xs text-[var(--text-tertiary)] mb-1">
                  Current file:
                </p>
                <p className="text-sm text-[var(--text-primary)] truncate font-mono">
                  {formatFileName(progress.currentFile)}
                </p>
                <p className="text-xs text-[var(--text-tertiary)] mt-0.5 truncate">
                  {progress.currentFile}
                </p>
              </div>
            )}

            {/* Stats grid */}
            <div className="grid grid-cols-2 gap-3">
              {progress.failed > 0 && (
                <div>
                  <p className="text-xs text-[var(--text-tertiary)] mb-1">
                    Failed
                  </p>
                  <p className="text-sm font-semibold text-[var(--error)]">
                    {progress.failed}
                  </p>
                </div>
              )}

              {isActive && progress.estimatedRemainingMs && (
                <div>
                  <p className="text-xs text-[var(--text-tertiary)] mb-1">
                    Time remaining
                  </p>
                  <p className="text-sm font-semibold text-[var(--text-primary)]">
                    {formatTime(progress.estimatedRemainingMs)}
                  </p>
                </div>
              )}
            </div>

            {/* Success message */}
            {progress.status === 'complete' && (
              <div className="mt-4 p-3 bg-[var(--success-light)]/20 border border-[var(--success-light)] rounded">
                <p className="text-sm text-[var(--success)]">
                  Successfully indexed {progress.processed} document{progress.processed !== 1 ? 's' : ''}
                  {progress.failed > 0 && ` (${progress.failed} failed)`}
                </p>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
