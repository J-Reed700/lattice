import { useEffect, useState } from 'react';

import { AlertTriangle, Check, Loader2, X } from 'lucide-react';

import type { CompactionRun } from '@/stores/compactionStore';

/** `42s`, `1m 05s`. */
export function formatElapsed(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
}

/** Milliseconds since `since`, ticking once a second; `0` when not running. */
function useElapsed(since: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (since === null) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [since]);
  return since === null ? 0 : now - since;
}

const DISMISS =
  'inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]';

/**
 * Where a `/compact` stands, said at the end of the thread where it was asked
 * for. Compacting runs the utility model over the older messages, a minute or
 * more, so it is shown running for as long as it runs, then what it did, or
 * why it could not, until the reader dismisses it.
 */
export function CompactionStatus({ run, onDismiss }: { run: CompactionRun; onDismiss: () => void }) {
  const [showSummary, setShowSummary] = useState(false);
  const elapsed = useElapsed(run.state === 'running' ? run.startedAt : null);

  if (run.state === 'running') {
    return (
      <div className="chat-beside-margin px-6 py-3" role="status" aria-live="polite">
        <div className="flex items-center gap-3 rounded-lg border border-[hsl(var(--accent)/0.3)] bg-[hsl(var(--accent)/0.06)] px-3.5 py-2.5">
          <Loader2 className="h-4 w-4 shrink-0 animate-spin text-[hsl(var(--accent))]" strokeWidth={2} aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-[hsl(var(--text-primary))]">Compacting context…</p>
            <p className="text-xs text-[hsl(var(--text-muted))]">
              Summarizing the older messages with the utility model. Sending waits until it’s done.
            </p>
          </div>
          <span className="shrink-0 text-xs tabular-nums text-[hsl(var(--text-muted))]">{formatElapsed(elapsed)}</span>
        </div>
      </div>
    );
  }

  const took = formatElapsed(run.finishedAt - run.startedAt);

  if (run.state === 'failed') {
    return (
      <div className="chat-beside-margin px-6 py-3" role="alert">
        <div className="flex items-start gap-3 rounded-lg border border-[hsl(var(--danger)/0.3)] bg-[hsl(var(--danger)/0.06)] px-3.5 py-2.5">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-[hsl(var(--danger))]" strokeWidth={2} aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-[hsl(var(--text-primary))]">Couldn’t compact the context</p>
            <p className="text-xs text-[hsl(var(--text-secondary))]">{run.error}</p>
          </div>
          <button type="button" onClick={onDismiss} aria-label="Dismiss" title="Dismiss" className={DISMISS}>
            <X className="h-3.5 w-3.5" strokeWidth={2} />
          </button>
        </div>
      </div>
    );
  }

  const { record } = run;
  const count = record.originalMessageCount;
  return (
    <div className="chat-beside-margin px-6 py-3" role="status" aria-live="polite">
      <div className="rounded-lg border border-[hsl(var(--border-default))] bg-[hsl(var(--surface-raised))] px-3.5 py-2.5">
        <div className="flex items-start gap-3">
          <Check className="mt-0.5 h-4 w-4 shrink-0 text-[hsl(var(--success))]" strokeWidth={2.2} aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-[hsl(var(--text-primary))]">Context compacted</p>
            <p className="text-xs text-[hsl(var(--text-muted))]">
              {count} older {count === 1 ? 'message' : 'messages'} summarized in {took}. Later turns read the
              summary; the messages stay here and stay searchable.
            </p>
            {record.summaryText.trim() && (
              <button
                type="button"
                onClick={() => setShowSummary((open) => !open)}
                aria-expanded={showSummary}
                className="mt-1 text-xs font-medium text-[hsl(var(--accent))] hover:text-[hsl(var(--accent-hover))]"
              >
                {showSummary ? 'Hide summary' : 'Show summary'}
              </button>
            )}
          </div>
          <button type="button" onClick={onDismiss} aria-label="Dismiss" title="Dismiss" className={DISMISS}>
            <X className="h-3.5 w-3.5" strokeWidth={2} />
          </button>
        </div>
        {showSummary && (
          <p className="mt-2 max-h-64 overflow-y-auto whitespace-pre-wrap border-t border-[hsl(var(--border-subtle))] pt-2 text-xs leading-relaxed text-[hsl(var(--text-secondary))]">
            {record.summaryText}
          </p>
        )}
      </div>
    </div>
  );
}
