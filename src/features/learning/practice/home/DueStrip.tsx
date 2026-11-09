import { useId } from 'react';

import { Clock, RefreshCw } from 'lucide-react';

import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import type * as Wire from '@/lib/bindings';

export interface DueStripProps {
  items: Wire.DueItemDto[] | undefined;
  loading?: boolean;
  error?: string | null;
  onRetry?: () => void;
  now?: number;
  /** Starts an item; the most overdue one for the button. */
  onStart: (item: Wire.DueItemDto) => void;
}

/** What is due from drills and coding, most overdue first, with one button for the first. */
export function DueStrip({ items, loading, error, onRetry, now = Date.now(), onStart }: DueStripProps) {
  const uid = useId();
  const list = items ?? [];
  const first = list[0];

  return (
    <section aria-labelledby={`${uid}-heading`} className="flex flex-col gap-3 rounded-xl border border-border-default bg-surface p-5">
      <h2 id={`${uid}-heading`} className="text-xs font-semibold uppercase tracking-wider text-text-muted">
        Due
      </h2>
      {loading && <StatusLine state={{ kind: 'busy', label: 'Checking what is due…' }} />}
      {error && <StatusLine state={{ kind: 'error', message: error, retry: onRetry }} />}
      {!loading && !error && list.length === 0 && <p className="text-sm text-text-secondary">Nothing is due. Problem patterns and drills come back here when it is time.</p>}
      {list.length > 0 && (
        <ul className="m-0 list-none divide-y divide-border-subtle p-0">
          {list.map((item) => {
            const overdue = item.dueAt < now;
            return (
              <li key={`${item.kind}:${item.lessonId ?? ''}:${item.material ?? ''}`} className="flex flex-wrap items-center justify-between gap-2 py-2 text-sm">
                <span className="font-medium text-text-primary">{item.label}</span>
                <span
                  className={`inline-flex min-h-7 items-center gap-1.5 rounded-full px-2.5 text-xs font-medium ${overdue ? 'bg-warning-muted text-warning-fg' : 'bg-surface-raised text-text-secondary'}`}
                >
                  {overdue ? <Clock className="h-3.5 w-3.5" aria-hidden="true" /> : <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />}
                  {overdue ? 'Overdue' : 'Due today'}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {first && (
        <div>
          <button
            type="button"
            onClick={() => onStart(first)}
            className="inline-flex min-h-11 items-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            Start the most overdue
          </button>
        </div>
      )}
    </section>
  );
}
