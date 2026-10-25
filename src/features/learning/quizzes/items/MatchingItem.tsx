import { PairingBoard } from '@/components/ui/PairingBoard';
import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark, partAt, PartWhy } from '@/features/learning/quizzes/items/ReviewParts';

/** Visual pairing keeps the existing response shape and grading behavior. */
export function MatchingEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  return <div>
    <PairingBoard leftItems={item.leftItems} rightItems={item.rightItems} matches={draft.matches} disabled={disabled} labelledBy={promptId} onChange={(matches) => onChange({ matches })} />
    {item.rightItems.length > item.leftItems.length && <p className="mt-3 text-xs text-text-muted">One answer on the right matches nothing.</p>}
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
