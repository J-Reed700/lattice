/**
 * InlineError Component
 *
 * Minimal error display for inline/compact error states.
 * Used for small components or tight spaces.
 */

import { AlertCircle, RefreshCw } from 'lucide-react';

export interface InlineErrorProps {
  error: Error;
  resetError?: () => void;
  message?: string;
  compact?: boolean;
}

export function InlineError({
  error,
  resetError,
  message,
  compact = false,
}: InlineErrorProps) {
  const displayMessage = message || error.message || 'An error occurred';

  if (compact) {
    return (
      <div className="flex items-center gap-2 px-3 py-2 bg-[var(--error-light)]/20 border border-[var(--error-light)] rounded-lg">
        <AlertCircle className="w-4 h-4 text-[var(--error)] flex-shrink-0" />
        <p className="text-sm text-[var(--error)] flex-1 min-w-0 truncate">
          {displayMessage}
        </p>
        {resetError && (
          <button
            onClick={resetError}
            className="flex-shrink-0 p-1 text-[var(--error)] hover:bg-[var(--error-light)] rounded transition-colors"
            aria-label="Retry"
            title="Retry"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
    );
  }

  return (
    <div className="border-l-4 border-[var(--error)] bg-[var(--error-light)]/20 p-4 rounded-r-lg">
      <div className="flex items-start gap-3">
        <div className="flex-shrink-0">
          <AlertCircle className="w-5 h-5 text-[var(--error)] mt-0.5" />
        </div>
        <div className="flex-1 min-w-0">
          <h4 className="text-sm font-semibold text-[var(--error)] mb-1">
            {error.name || 'Error'}
          </h4>
          <p className="text-sm text-[var(--error)] break-words">
            {displayMessage}
          </p>
          {resetError && (
            <button
              onClick={resetError}
              className="inline-flex items-center gap-1.5 mt-3 px-3 py-1.5 text-sm font-medium text-[var(--error)] hover:text-[var(--error)] hover:opacity-80 bg-[var(--error-light)]/40 hover:bg-[var(--error-light)] rounded-lg transition-colors"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              Try again
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Compact variant for very tight spaces
 */
export function CompactError({ error, resetError }: InlineErrorProps) {
  return <InlineError error={error} resetError={resetError} compact />;
}
