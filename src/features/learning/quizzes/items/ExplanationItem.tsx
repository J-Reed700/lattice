import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation } from '@/features/learning/quizzes/items/ReviewParts';
import { points } from '@/features/learning/quizzes/quizCopy';

export function wordCount(text: string): number {
  const trimmed = text.trim();
  return trimmed ? trimmed.split(/\s+/).length : 0;
}

/** A written answer graded against a rubric the learner can see. No minimum length. */
export function ExplanationEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  const fieldId = `${item.id}-explanation`;
  const words = wordCount(draft.textResponse);
  const criteria = item.rubric.map((criterion) => criterion.title.toLowerCase());
  return <div>
    {criteria.length > 0 && <p id={`${fieldId}-rubric`} className="mb-3 rounded-xl bg-background/70 px-4 py-2.5 text-xs leading-5 text-text-secondary">
      <span className="font-semibold text-text-primary">Graded on:</span> {criteria.join(', ')} · {points(item.points)} points, by your model, with reasons you can read.
    </p>}
    <label htmlFor={fieldId} className="text-xs font-semibold text-text-secondary">Your explanation</label>
    <textarea id={fieldId} rows={7} value={draft.textResponse} disabled={disabled} aria-describedby={`${promptId} ${fieldId}-rubric ${fieldId}-count`}
      onChange={(event) => onChange({ textResponse: event.target.value })}
      className="mt-1.5 w-full resize-y rounded-xl border border-border bg-background px-4 py-3 text-base leading-7 text-text-primary outline-hidden transition focus:border-accent focus:ring-2 focus:ring-accent/30 disabled:opacity-60" />
    <p id={`${fieldId}-count`} className="mt-1.5 text-right text-xs tabular-nums text-text-muted">{words} {words === 1 ? 'word' : 'words'} · no minimum</p>
  </div>;
}

/** Without a rationale (still grading), the answer as written; with one, the panel shows it highlighted. */
export function ExplanationReview({ draft, reveal, result }: ReviewProps) {
  return <div>
    {!result?.rationale && <blockquote className="whitespace-pre-wrap rounded-xl border-l-2 border-accent/40 bg-background/60 px-4 py-3 text-sm leading-6 text-text-secondary">
      {draft.textResponse.trim() || 'No answer given.'}
    </blockquote>}
    <KeyExplanation text={reveal.explanation} label="What a strong answer covers" />
  </div>;
}
