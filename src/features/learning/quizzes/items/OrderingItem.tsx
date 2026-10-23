import { useState, type KeyboardEvent } from 'react';

import { ArrowDown, ArrowUp } from 'lucide-react';

import { shownOrder } from '@/features/learning/quizzes/items/itemResponse';
import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark } from '@/features/learning/quizzes/items/ReviewParts';

const smallButton = 'grid h-11 w-11 place-items-center rounded-lg border border-border text-text-secondary hover:bg-background focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-35';

/** A list with Move up / Move down buttons, and Alt+↑/↓ on a focused row. No dragging needed. */
export function OrderingEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  const order = shownOrder(item, draft);
  const [announcement, setAnnouncement] = useState('');
  const move = (from: number, to: number, focusRow: boolean) => {
    if (to < 0 || to >= order.length || disabled) return;
    const next = [...order];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    onChange({ orderedValues: next });
    setAnnouncement(`${moved} moved to position ${to + 1} of ${order.length}.`);
    if (focusRow) requestAnimationFrame(() => document.getElementById(`${item.id}-row-${to}`)?.focus());
  };
  const onRowKeyDown = (event: KeyboardEvent<HTMLLIElement>, index: number) => {
    if (!event.altKey || (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) return;
    event.preventDefault();
    move(index, index + (event.key === 'ArrowUp' ? -1 : 1), true);
  };
  return <div>
    <p id={`${item.id}-order-hint`} className="mb-3 text-xs text-text-muted">Put these in order, first at the top. Use Move up and Move down, or Alt+↑ and Alt+↓ on a row.</p>
    <ol aria-labelledby={promptId} aria-describedby={`${item.id}-order-hint`} className="space-y-2">{order.map((value, index) => (
      <li key={value} id={`${item.id}-row-${index}`} tabIndex={0} onKeyDown={(event) => onRowKeyDown(event, index)}
        aria-label={`${index + 1} of ${order.length}: ${value}`}
        className="flex items-center gap-3 rounded-xl border border-border bg-background/40 py-1.5 pl-4 pr-1.5 text-sm leading-6 text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">
        <span aria-hidden="true" className="w-5 shrink-0 font-serif text-base tabular-nums text-accent">{index + 1}</span>
        <span className="min-w-0 flex-1">{value}</span>
        <button type="button" aria-label={`Move ${value} up`} disabled={disabled || index === 0} onClick={() => move(index, index - 1, false)} className={smallButton}><ArrowUp size={16} /></button>
        <button type="button" aria-label={`Move ${value} down`} disabled={disabled || index === order.length - 1} onClick={() => move(index, index + 1, false)} className={smallButton}><ArrowDown size={16} /></button>
      </li>))}
    </ol>
    {!draft.orderedValues.length && <button type="button" disabled={disabled} onClick={() => onChange({ orderedValues: [...order] })}
      className="mt-3 min-h-11 rounded-full border border-accent/40 px-4 text-sm font-medium text-accent hover:bg-accent/5 disabled:opacity-50">Keep this order as my answer</button>}
    <p role="status" aria-live="polite" className="sr-only">{announcement}</p>
  </div>;
}

/** Your order beside the correct one, each position said in words. */
export function OrderingReview({ item, draft, reveal }: ReviewProps) {
  const yours = shownOrder(item, draft);
  return <div>
    <div className="grid gap-4 sm:grid-cols-2">
      <section aria-label="Your order"><h5 className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Your order</h5>
        <ol className="mt-2 space-y-1.5">{yours.map((value, index) => <li key={value} className="flex flex-wrap items-center gap-2 rounded-lg border border-border px-3 py-2 text-sm">
          <span className="w-4 font-serif tabular-nums text-text-muted">{index + 1}</span><span className="min-w-0 flex-1 text-text-secondary">{value}</span>
          <PartMark state={reveal.orderedValues[index] === value ? 'right' : 'wrong'}>{reveal.orderedValues[index] === value ? 'In place' : `Belongs at ${reveal.orderedValues.indexOf(value) + 1}`}</PartMark>
        </li>)}</ol>
      </section>
      <section aria-label="Correct order"><h5 className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Correct order</h5>
        <ol className="mt-2 space-y-1.5">{reveal.orderedValues.map((value, index) => <li key={value} className="flex gap-2 rounded-lg bg-emerald-500/5 px-3 py-2 text-sm">
          <span className="w-4 font-serif tabular-nums text-emerald-700 dark:text-emerald-300">{index + 1}</span><span className="text-text-primary">{value}</span>
        </li>)}</ol>
      </section>
    </div>
    <KeyExplanation text={reveal.explanation} />
  </div>;
}
