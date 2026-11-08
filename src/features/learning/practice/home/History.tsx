import { useState } from 'react';
import { ArrowRight, CheckCircle2, Clock } from 'lucide-react';

export type HistoryFilter =
  | 'all'
  | 'explain'
  | 'spot'
  | 'code'
  | 'mock'
  | 'role';

export interface HistoryItem {
  id: string;
  when: string;
  timestamp?: number;
  what: string;
  kind: 'explain' | 'spot' | 'code' | 'mock' | 'role' | string;
  kindLabel?: string;
  settingOrHelp: string;
  result: string;
  resultBadge?: string;
  details?: string;
}

export interface HistoryProps {
  programId: string;
  items?: HistoryItem[];
  onOpenItem?: (item: HistoryItem) => void;
  className?: string;
}

const FILTERS: { id: HistoryFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'explain', label: 'Explain' },
  { id: 'spot', label: 'Spot the mistakes' },
  { id: 'code', label: 'Code' },
  { id: 'mock', label: 'Mock interview' },
  { id: 'role', label: 'Role-play' },
];

export function History({
  items = [],
  onOpenItem,
  className,
}: HistoryProps): JSX.Element {
  const [filter, setFilter] = useState<HistoryFilter>('all');

  const filteredItems = items.filter((item) => {
    if (filter === 'all') return true;
    return item.kind.toLowerCase() === filter.toLowerCase();
  });

  return (
    <section
      id="history"
      aria-labelledby="history-heading"
      className={`space-y-4 ${className ?? ''}`}
    >
      <h2
        id="history-heading"
        className="font-serif text-2xl font-semibold text-text-primary"
      >
        History
      </h2>

      <fieldset className="m-0 border-0 p-0">
        <legend className="sr-only">Filter history</legend>
        <div className="flex flex-wrap gap-2">
          {FILTERS.map((f) => {
            const isActive = filter === f.id;
            return (
              <button
                key={f.id}
                type="button"
                aria-pressed={isActive}
                onClick={() => setFilter(f.id)}
                className={`inline-flex min-h-[44px] cursor-pointer items-center rounded-full px-4 text-sm font-medium transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
                  isActive
                    ? 'border border-border-default bg-surface-raised font-semibold text-text-primary shadow-xs'
                    : 'border border-border-subtle bg-surface text-text-secondary hover:text-text-primary'
                }`}
              >
                {f.label}
              </button>
            );
          })}
        </div>
      </fieldset>

      <div className="overflow-x-auto rounded-xl border border-border-default bg-surface shadow-xs">
        <table className="w-full min-w-[700px] border-collapse text-left text-sm">
          <caption className="sr-only">
            Past sessions, attempts and drills
          </caption>
          <thead>
            <tr className="border-b border-border-default text-xs font-semibold uppercase tracking-wider text-text-tertiary">
              <th scope="col" className="px-4 py-3">
                When
              </th>
              <th scope="col" className="px-4 py-3">
                What
              </th>
              <th scope="col" className="px-4 py-3">
                Kind
              </th>
              <th scope="col" className="px-4 py-3">
                Setting or help
              </th>
              <th scope="col" className="px-4 py-3">
                Result
              </th>
              <th scope="col" className="px-4 py-3 text-right">
                <span className="sr-only">Open</span>
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border-subtle text-text-primary">
            {filteredItems.length > 0 ? (
              filteredItems.map((item) => (
                <tr
                  key={item.id}
                  className="transition-colors hover:bg-surface-raised"
                >
                  <td className="whitespace-nowrap px-4 py-3.5 text-text-secondary">
                    {item.when}
                  </td>
                  <td className="px-4 py-3.5 font-semibold">
                    {item.what}
                  </td>
                  <td className="px-4 py-3.5 capitalize text-text-secondary">
                    {item.kindLabel ?? item.kind}
                  </td>
                  <td className="px-4 py-3.5 text-text-secondary">
                    {item.settingOrHelp}
                  </td>
                  <td className="px-4 py-3.5">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="inline-flex min-h-[26px] items-center gap-1.5 rounded-full bg-success-muted px-2.5 text-xs font-medium text-success-fg">
                        <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />
                        <span>{item.result}</span>
                      </span>
                      {item.details && (
                        <span className="text-xs text-text-secondary">
                          {item.details}
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="px-4 py-3.5 text-right">
                    <button
                      type="button"
                      onClick={() => onOpenItem?.(item)}
                      className="inline-flex min-h-[44px] cursor-pointer items-center gap-1 rounded-lg px-2 text-sm font-medium text-text-secondary hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <span>Open</span>
                      <span className="sr-only"> {item.what}</span>
                      <ArrowRight className="h-4 w-4" aria-hidden="true" />
                    </button>
                  </td>
                </tr>
              ))
            ) : (
              <tr>
                <td
                  colSpan={6}
                  className="px-4 py-8 text-center text-text-muted"
                >
                  No past items match this filter.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}
