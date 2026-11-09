import { AlertCircle, Loader2, RotateCcw } from 'lucide-react';

import { cn } from '@/lib/utils';

export type StatusState =
  | { kind: 'idle' }
  | { kind: 'busy'; label: string }
  | { kind: 'error'; message: string; retry?: () => void; reload?: { label: string; run: () => void } };

/** A busy line or an error with Try again, announced to screen readers. */
export function StatusLine({ state, className }: { state: StatusState; className?: string }) {
  if (state.kind === 'idle') return null;

  if (state.kind === 'busy') {
    return (
      <div role="status" aria-live="polite" className={cn('flex items-center gap-2 text-xs text-text-secondary', className)}>
        <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-accent" aria-hidden="true" />
        <span>{state.label}</span>
      </div>
    );
  }

  return (
    <div
      role="alert"
      className={cn(
        'flex flex-wrap items-center justify-between gap-2 rounded-lg bg-danger-muted px-3 py-2 text-xs text-danger-fg',
        className,
      )}
    >
      <span className="flex items-center gap-2">
        <AlertCircle className="h-4 w-4 shrink-0" aria-hidden="true" />
        <span>{state.message}</span>
      </span>
      {state.reload ? (
        <button
          type="button"
          onClick={state.reload.run}
          className="inline-flex items-center gap-1 font-medium underline focus:outline-hidden focus:ring-2 focus:ring-ring"
        >
          <RotateCcw className="h-3 w-3" aria-hidden="true" />
          {state.reload.label}
        </button>
      ) : (
        state.retry && (
        <button
          type="button"
          onClick={state.retry}
          className="inline-flex items-center gap-1 font-medium underline focus:outline-hidden focus:ring-2 focus:ring-ring"
        >
          <RotateCcw className="h-3 w-3" aria-hidden="true" />
          Try again
        </button>
        )
      )}
    </div>
  );
}
