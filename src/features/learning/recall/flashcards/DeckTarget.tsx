import { useState } from 'react';

import { CalendarClock } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { SectionHeading } from '@/components/ui/PageHeader';
import { fromNow, prepPlan } from '@/features/learning/recall/flashcards/retrieval';
import { useSetFlashcardDeckTarget } from '@/features/learning/recall/flashcards/useFlashcards';
import type { StudyDeckDto } from '@/lib/bindings';

/** `yyyy-MM-ddTHH:mm` in local time, the value a datetime-local input takes. */
function localInput(at: number) {
  const date = new Date(at - new Date(at).getTimezoneOffset() * 60_000);
  return date.toISOString().slice(0, 16);
}

function when(at: number) {
  return new Date(at).toLocaleString(undefined, { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
}

/**
 * When the deck must be remembered by, such as an interview, and the sessions
 * to fit in before it. With a target set, reviews are spaced to land before it.
 */
export function DeckTarget({ deck, now }: { deck: StudyDeckDto; now: number }) {
  const save = useSetFlashcardDeckTarget();
  const target = deck.targetAt ?? null;
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(() => localInput(target ?? now + 24 * 3_600_000));
  const nextDue = deck.cards.filter(card => card.dueAt > now).reduce<number | null>((soonest, card) => soonest === null || card.dueAt < soonest ? card.dueAt : soonest, null);
  const submit = (targetAt: number | null) => save.mutate({ deckId: deck.id, targetAt }, { onSuccess: () => setEditing(false) });
  const parsed = value ? new Date(value).getTime() : NaN;
  const active = target !== null && target > now;

  return <section aria-labelledby="deck-target-heading" className="mb-8 rounded-xl border border-border-subtle p-5">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="flex items-start gap-3">
        <CalendarClock size={18} className="mt-0.5 shrink-0 text-accent" aria-hidden="true" />
        <div>
          <h2 id="deck-target-heading" className="text-sm font-medium text-text-primary">{active ? `Interview or exam ${when(target)}` : 'Interview or exam date'}</h2>
          <p className="mt-1 text-xs leading-5 text-text-muted">{active
            ? `${fromNow(target, now).replace(/^in /, 'In ')}. Reviews are spaced to land before it, closer together as it nears.`
            : target !== null ? 'That date has passed, so cards are back on their long-term schedule.'
              : 'Set when you need to remember this. Reviews are then spaced to fit the time you have, instead of days apart.'}</p>
        </div>
      </div>
      {!editing && <div className="flex gap-2">
        <Button size="sm" variant={active ? 'ghost' : 'secondary'} onClick={() => setEditing(true)}>{active ? 'Change' : 'Set date'}</Button>
        {target !== null && <Button size="sm" variant="ghost" disabled={save.isPending} onClick={() => submit(null)}>Clear</Button>}
      </div>}
    </div>
    {editing && <form className="mt-4 flex flex-wrap items-end gap-2" onSubmit={event => { event.preventDefault(); if (Number.isFinite(parsed)) submit(parsed); }}>
      <label className="text-xs text-text-muted">When is it?
        <input type="datetime-local" value={value} min={localInput(now)} onChange={event => setValue(event.target.value)} className="mt-1 block rounded-md border border-border-subtle bg-background px-3 py-2 text-sm text-text-primary" />
      </label>
      <Button size="sm" type="submit" disabled={save.isPending || !Number.isFinite(parsed) || parsed <= now}>{save.isPending ? 'Saving…' : 'Save'}</Button>
      <Button size="sm" type="button" variant="ghost" disabled={save.isPending} onClick={() => setEditing(false)}>Cancel</Button>
    </form>}
    {save.isError && <p role="alert" className="mt-3 text-sm text-danger-fg">{save.error.message}</p>}
    {active && !editing && <div className="mt-6">
      <SectionHeading>Your plan</SectionHeading>
      <ol className="space-y-3">{prepPlan(now, target, nextDue).map(step => <li key={step.title} className="grid grid-cols-[7.5rem_1fr] gap-3 text-sm">
        <span className="text-xs leading-5 text-text-muted">{step.at <= now + 60_000 ? 'Now' : when(step.at)}</span>
        <span><span className="font-medium text-text-primary">{step.title}</span><span className="block text-xs leading-5 text-text-secondary">{step.detail}</span></span>
      </li>)}</ol>
    </div>}
  </section>;
}
