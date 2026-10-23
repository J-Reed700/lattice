import { Check } from 'lucide-react';

import { optionLetter, type EditorProps, type ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark, type PartState } from '@/features/learning/quizzes/items/ReviewParts';

/** A checkbox group: Tab moves between options, Space or Enter ticks one. */
export function MultiSelectEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  const toggle = (index: number) => {
    const picked = draft.selectedIndices.includes(index);
    onChange({ selectedIndices: picked ? draft.selectedIndices.filter((value) => value !== index) : [...draft.selectedIndices, index] });
  };
  return <div role="group" aria-labelledby={promptId} aria-describedby={`${promptId}-hint`}>
    <p id={`${promptId}-hint`} className="mb-3 text-xs font-medium text-text-muted">Choose all that apply.</p>
    <div className="space-y-2.5">{item.options.map((option, index) => {
      const checked = draft.selectedIndices.includes(index);
      return <button key={`${index}-${option}`} type="button" role="checkbox" aria-checked={checked} disabled={disabled} onClick={() => toggle(index)}
        className={`flex min-h-11 w-full items-start gap-3 rounded-xl border px-4 py-3 text-left text-sm leading-6 transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-60 ${checked ? 'border-accent bg-accent/5 text-text-primary ring-1 ring-accent/30' : 'border-border text-text-secondary hover:bg-background'}`}>
        <span aria-hidden="true" className={`mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-md border ${checked ? 'border-accent bg-accent text-accent-fg' : 'border-border'}`}>{checked && <Check size={13} />}</span>
        <span className="min-w-0"><span aria-hidden="true" className="mr-1.5 font-semibold text-text-muted">{optionLetter(index)}.</span>{option}</span>
      </button>;
    })}</div>
  </div>;
}

function pickState(picked: boolean, key: boolean): { state: PartState; words: string } | null {
  if (picked && key) return { state: 'right', words: 'You picked it · in the key' };
  if (picked) return { state: 'wrong', words: 'You picked it · not in the key' };
  if (key) return { state: 'missed', words: 'Missed · in the key' };
  return null;
}

/** Each option says whether you picked it and whether the key holds it, with the reason for a wrong pick. */
export function MultiSelectReview({ item, draft, reveal }: ReviewProps) {
  return <div>
    <ul className="space-y-2">{item.options.map((option, index) => {
      const mark = pickState(draft.selectedIndices.includes(index), reveal.correctIndices.includes(index));
      const why = mark?.state === 'wrong' ? reveal.distractorRationales[index]?.trim() : '';
      return <li key={`${index}-${option}`} className="rounded-xl border border-border px-4 py-2.5 text-sm leading-6">
        <div className="flex flex-wrap items-start gap-3">
          <span className="min-w-0 flex-1 text-text-secondary"><span className="mr-1.5 font-semibold text-text-muted">{optionLetter(index)}.</span>{option}</span>
          {mark && <PartMark state={mark.state}>{mark.words}</PartMark>}
        </div>
        {why && <p className="mt-1 text-xs leading-5 text-text-secondary">{why}</p>}
      </li>;
    })}</ul>
    <KeyExplanation text={reveal.explanation} />
  </div>;
}
