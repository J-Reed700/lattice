import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark } from '@/features/learning/quizzes/items/ReviewParts';
import type { LearningAnswerHintDto } from '@/lib/bindings';

/** What the input may say before submission: the key's kind and unit, never the key. */
export function answerHintWords(hint: LearningAnswerHintDto | null): string {
  if (hint?.numeric) return hint.unitHint ? `A number, in ${hint.unitHint}. Writing the unit is optional.` : 'A number.';
  return 'A word or a short phrase. Capitals, spacing and punctuation don\u2019t matter.';
}

export function ShortAnswerEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  const inputId = `${item.id}-answer`;
  return <div className="max-w-xl">
    <label htmlFor={inputId} className="text-xs font-semibold text-text-secondary">Your answer</label>
    <input id={inputId} type="text" inputMode={item.answerHint?.numeric && !item.answerHint.unitHint ? 'decimal' : undefined} autoComplete="off" spellCheck value={draft.textResponse} disabled={disabled}
      aria-describedby={`${promptId} ${inputId}-hint`} onChange={(event) => onChange({ textResponse: event.target.value })}
      className="mt-1.5 min-h-11 w-full rounded-xl border border-border bg-background px-4 text-base text-text-primary outline-hidden transition focus:border-accent focus:ring-2 focus:ring-accent/30 disabled:opacity-60" />
    <p id={`${inputId}-hint`} className="mt-2 text-xs leading-5 text-text-muted">{answerHintWords(item.answerHint)}</p>
  </div>;
}

export function ShortAnswerReview({ draft, reveal, result }: ReviewProps) {
  const right = result ? result.pointsEarned >= result.pointsPossible : undefined;
  return <div>
    <dl className="grid gap-2 text-sm leading-6 sm:grid-cols-[9rem_minmax(0,1fr)]">
      <dt className="text-text-muted">Your answer</dt>
      <dd className="flex flex-wrap items-center gap-2 text-text-primary">{draft.textResponse.trim() ? <span>“{draft.textResponse.trim()}”</span> : <span className="text-text-muted">No answer given</span>}
        {right === true && <PartMark state="right">Accepted</PartMark>}
        {right === false && result?.gradeStatus !== 'pending' && <PartMark state="wrong">Not accepted</PartMark>}
      </dd>
      {reveal.acceptedAnswer && <><dt className="text-text-muted">Accepted answer</dt><dd className="font-medium text-text-primary">{reveal.acceptedAnswer}</dd></>}
    </dl>
    <KeyExplanation text={reveal.explanation} />
  </div>;
}
