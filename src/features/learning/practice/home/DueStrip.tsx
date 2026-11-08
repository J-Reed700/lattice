import { Clock, RefreshCw } from 'lucide-react';

import type { DueItemDto } from './practiceHomeApi';

export interface DueStripProps {
  programId: string;
  items?: DueItemDto[];
  onStartMostOverdue?: () => void;
  className?: string;
}

export function DueStrip({
  items = [],
  onStartMostOverdue,
  className,
}: DueStripProps): JSX.Element {
  const overdueCount = items.filter((item) => item.isOverdue).length;

  return (
    <section
      aria-labelledby="due-heading"
      className={`flex flex-col gap-3 rounded-xl border border-border-default bg-surface p-5 shadow-xs ${className ?? ''}`}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div
            id="due-heading-sub"
            className="text-xs font-semibold uppercase tracking-wider text-text-tertiary"
          >
            Due
          </div>
          <h2
            id="due-heading"
            className="mt-0.5 font-serif text-lg font-medium text-text-primary"
          >
            {items.length > 0
              ? `${items.length} ${items.length === 1 ? 'thing' : 'things'} to come back to`
              : 'Up to date'}
          </h2>
        </div>
      </div>

      {items.length > 0 ? (
        <ul className="m-0 list-none divide-y divide-border-subtle p-0">
          {items.map((item) => (
            <li
              key={item.id}
              className="flex flex-wrap items-center justify-between gap-3 py-3 text-sm"
            >
              <div className="min-w-0">
                <div className="font-semibold text-text-primary">
                  {item.title}
                </div>
                {item.subtitle && (
                  <div className="text-xs text-text-secondary">
                    {item.subtitle}
                  </div>
                )}
              </div>
              <span
                className={`inline-flex min-h-[28px] items-center gap-1.5 rounded-full px-2.5 text-xs font-medium ${
                  item.isOverdue
                    ? 'bg-warning-muted text-warning-fg'
                    : 'bg-surface-raised text-text-secondary'
                }`}
              >
                {item.isOverdue ? (
                  <>
                    <Clock className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>Overdue</span>
                  </>
                ) : (
                  <>
                    <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>Due today</span>
                  </>
                )}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-sm text-text-secondary">
          No problem patterns or drills are due for review right now.
        </p>
      )}

      <p className="text-xs text-text-tertiary">
        Patterns stay unnamed until your debrief, so a problem never gives its
        approach away.
      </p>

      {items.length > 0 && (
        <div className="pt-1">
          <button
            type="button"
            onClick={onStartMostOverdue}
            className="inline-flex min-h-[44px] cursor-pointer items-center justify-center rounded-lg bg-accent px-4.5 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            Start the most overdue
          </button>
        </div>
      )}
    </section>
  );
}
