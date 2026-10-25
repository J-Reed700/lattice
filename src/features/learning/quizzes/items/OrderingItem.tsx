import { Button } from '@/components/ui/button';
import { SortableList } from '@/components/ui/SortableList';
import { shownOrder } from '@/features/learning/quizzes/items/itemResponse';
import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark, partAt, PartWhy } from '@/features/learning/quizzes/items/ReviewParts';

/** Sorting saves the same ordered response regardless of pointer or keyboard input. */
export function OrderingEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  const order = shownOrder(item, draft);
  return <div>
    <SortableList values={order} disabled={disabled} labelledBy={promptId} onReorder={(next) => onChange({ orderedValues: next })} />
    {!draft.orderedValues.length && <Button type="button" variant="outline" disabled={disabled} onClick={() => onChange({ orderedValues: [...order] })} className="mt-3 min-h-11 text-accent">Keep this order as my answer</Button>}
  </div>;
}

/** Your order beside the correct one, each position said in words. */
export function OrderingReview({ item, draft, reveal, result }: ReviewProps) {
  const yours = shownOrder(item, draft);
  return <div>
    <div className="grid gap-4 sm:grid-cols-2">
      <section aria-label="Your order"><h5 className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Your order</h5>
        <ol className="mt-2 space-y-1.5">{yours.map((value, index) => {
          const part = partAt(result, index);
          const inPlace = part ? part.correct : reveal.orderedValues[index] === value;
          return <li key={value} className="rounded-lg border border-border px-3 py-2 text-sm">
            <div className="flex flex-wrap items-center gap-2">
              <span className="w-4 font-serif tabular-nums text-text-muted">{index + 1}</span><span className="min-w-0 flex-1 text-text-secondary">{value}</span>
              <PartMark state={inPlace ? 'right' : 'wrong'}>{inPlace ? 'In place' : `Belongs at ${reveal.orderedValues.indexOf(value) + 1}`}</PartMark>
            </div>
            {!inPlace && <PartWhy text={part?.detail} />}
          </li>;
        })}</ol>
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
