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
      <div className="flex items-center gap-2 px-3 py-2 bg-[hsl(var(--danger-muted))] border border-[hsl(var(--danger-muted))] rounded-md">
        <AlertCircle className="w-4 h-4 text-[hsl(var(--danger-fg))] flex-shrink-0" strokeWidth={1.75} />
        <p className="text-sm text-[hsl(var(--danger-fg))] flex-1 min-w-0 truncate">
          {displayMessage}
        </p>
        {resetError && (
          <button
            onClick={resetError}
            className="flex-shrink-0 p-1 text-[hsl(var(--danger-fg))] hover:opacity-90 rounded transition-opacity duration-fast"
            aria-label="Retry"
            title="Retry"
          >
            <RefreshCw className="w-3.5 h-3.5" strokeWidth={1.75} />
          </button>
        )}
      </div>
    );
  }

  return (
    <div className="border-l-4 border-[hsl(var(--danger-fg))] bg-[hsl(var(--danger-muted))] p-4 rounded-r-md">
      <div className="flex items-start gap-3">
        <div className="flex-shrink-0">
          <AlertCircle className="w-5 h-5 text-[hsl(var(--danger-fg))] mt-0.5" strokeWidth={1.75} />
        </div>
        <div className="flex-1 min-w-0">
          <h4 className="text-sm font-semibold text-[hsl(var(--danger-fg))] mb-1">
            {error.name || 'Error'}
          </h4>
          <p className="text-sm text-[hsl(var(--danger-fg))] break-words">
            {displayMessage}
          </p>
          {resetError && (
            <button
              onClick={resetError}
              className="inline-flex items-center gap-1.5 mt-3 px-3 py-1.5 text-sm font-medium text-[hsl(var(--danger-fg))] hover:opacity-90 bg-[hsl(var(--danger-muted))] rounded-md transition-opacity duration-fast"
            >
              <RefreshCw className="w-3.5 h-3.5" strokeWidth={1.75} />
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
