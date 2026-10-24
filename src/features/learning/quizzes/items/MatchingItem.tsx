import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark, partAt, PartWhy } from '@/features/learning/quizzes/items/ReviewParts';

/** One native select per entry: fully keyboard operable, no dragging. */
export function MatchingEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  const setMatch = (index: number, value: string) => {
    const next = item.leftItems.map((_, i) => draft.matches[i] ?? null);
    next[index] = value === '' ? null : Number(value);
    onChange({ matches: next });
  };
  const spare = item.rightItems.length > item.leftItems.length;
  return <div role="group" aria-labelledby={promptId}>
    <ul className="space-y-2.5">{item.leftItems.map((left, index) => {
      const selectId = `${item.id}-match-${index}`;
      return <li key={`${index}-${left}`} className="grid items-center gap-2 rounded-xl border border-border bg-background/40 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] sm:gap-4">
        <label htmlFor={selectId} className="text-sm font-medium leading-6 text-text-primary">{left}</label>
        <select id={selectId} value={draft.matches[index] ?? ''} disabled={disabled} onChange={(event) => setMatch(index, event.target.value)}
          className="min-h-11 w-full rounded-lg border border-border bg-surface px-3 text-sm text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-60">
          <option value="">Choose…</option>
          {item.rightItems.map((right, rightIndex) => <option key={`${rightIndex}-${right}`} value={rightIndex}>{right}</option>)}
        </select>
      </li>;
    })}</ul>
    {spare && <p className="mt-3 text-xs text-text-muted">One answer on the right matches nothing.</p>}
  </div>;
}

/** Each pair: your match against the key, said in words. */
export function MatchingReview({ item, draft, reveal, result }: ReviewProps) {
  return <div>
    <ul className="space-y-2">{item.leftItems.map((left, index) => {
      const yours = draft.matches[index];
      const key = reveal.matches[index];
      const part = partAt(result, index);
      const right = part ? part.correct : yours !== null && yours !== undefined && yours === key;
      return <li key={`${index}-${left}`} className="rounded-xl border border-border px-4 py-2.5 text-sm leading-6">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span className="font-semibold text-text-primary">Pair {index + 1}</span>
          <PartMark state={right ? 'right' : 'wrong'}>{right ? 'Correct' : 'Not correct'}</PartMark>
        </div>
        <p className="mt-1 text-text-secondary">
          <span className="font-medium text-text-primary">{left}</span> matches <span className="font-medium text-text-primary">{key !== undefined ? item.rightItems[key] : '—'}</span>
          {!right && (yours === null || yours === undefined ? '; you left it unmatched' : <>, not <span className="italic">{item.rightItems[yours]}</span></>)}.
        </p>
        {!right && <PartWhy text={part?.detail} />}
      </li>;
    })}</ul>
    <KeyExplanation text={reveal.explanation} />
  </div>;
}
