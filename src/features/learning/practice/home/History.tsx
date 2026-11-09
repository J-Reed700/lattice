import { useId, useState } from 'react';

import { ArrowRight, CircleCheck, CircleDashed, Layers } from 'lucide-react';

import { StatusLine } from '@/features/learning/practice/shared/StatusLine';

import { CARD_LABELS, type HistoryItem } from './historyItems';
import type { CardId } from './practiceChoices';

type Filter = 'all' | CardId;

const FILTERS: { id: Filter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'explain', label: CARD_LABELS.explain },
  { id: 'spot', label: CARD_LABELS.spot },
  { id: 'code', label: CARD_LABELS.code },
  { id: 'mock', label: CARD_LABELS.mock },
  { id: 'role', label: CARD_LABELS.role },
];

export interface HistoryProps {
  items: HistoryItem[];
  loading?: boolean;
  error?: string | null;
  onRetry?: () => void;
  onOpen: (item: HistoryItem) => void;
  /** Opens the practical workbench as a whole, for activities with no card of their own. */
  onOpenWorkbench: () => void;
}

/** Past sessions, attempts and drills in one list, with a filter for each card. */
export function History({ items, loading, error, onRetry, onOpen, onOpenWorkbench }: HistoryProps) {
  const uid = useId();
  const [filter, setFilter] = useState<Filter>('all');
  const shown = items.filter((item) => filter === 'all' || item.card === filter);

  return (
    <section aria-labelledby={`${uid}-heading`} className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 id={`${uid}-heading`} className="font-serif text-xl font-semibold text-text-primary">
          History
        </h2>
        <button
          type="button"
          onClick={onOpenWorkbench}
          className="inline-flex min-h-11 items-center gap-1.5 rounded-lg border border-border-default px-3 text-sm font-medium text-text-primary hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Layers className="h-4 w-4" aria-hidden="true" />
          Code labs and simulations
        </button>
      </div>

      <div role="group" aria-label="Filter history" className="flex flex-wrap gap-2">
        {FILTERS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            aria-pressed={filter === entry.id}
            onClick={() => setFilter(entry.id)}
            className={`inline-flex min-h-11 items-center rounded-full border px-4 text-sm font-medium focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
              filter === entry.id ? 'border-accent bg-accent-muted text-accent' : 'border-border-default bg-surface text-text-secondary hover:text-text-primary'
            }`}
          >
            {entry.label}
          </button>
        ))}
      </div>

      {loading && <StatusLine state={{ kind: 'busy', label: 'Loading your history…' }} />}
      {error && <StatusLine state={{ kind: 'error', message: error, retry: onRetry }} />}
      {!loading && !error && shown.length === 0 && (
        <p className="rounded-xl border border-dashed border-border-default p-6 text-sm text-text-secondary">
          {items.length === 0 ? 'Nothing here yet. Start something above and it will be listed.' : 'Nothing under this filter.'}
        </p>
      )}
      {shown.length > 0 && (
        <ul className="m-0 list-none space-y-2 p-0">
          {shown.map((item) => (
            <li key={item.id} className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border-default bg-surface p-4">
              <div className="min-w-0 flex-1">
                <div className="text-xs font-medium text-text-muted">
                  {CARD_LABELS[item.card]} · {new Date(item.when).toLocaleDateString()}
                </div>
                <div className="truncate text-sm font-semibold text-text-primary">{item.title}</div>
                <div className="text-sm text-text-secondary">{item.detail}</div>
              </div>
              <span className="inline-flex min-h-7 items-center gap-1.5 rounded-full bg-surface-raised px-2.5 text-xs font-medium text-text-secondary">
                {item.resumable ? <CircleDashed className="h-3.5 w-3.5" aria-hidden="true" /> : <CircleCheck className="h-3.5 w-3.5" aria-hidden="true" />}
                {item.result}
              </span>
              <button
                type="button"
                aria-label={`Open ${item.title}`}
                onClick={() => onOpen(item)}
                className="inline-flex min-h-11 items-center gap-1 rounded-lg px-2 text-sm font-medium text-text-secondary hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                Open
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
