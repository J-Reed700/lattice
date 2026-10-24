import type { KeyboardEvent } from 'react';

import { optionLetter, type EditorProps, type ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark, ReasonLine } from '@/features/learning/quizzes/items/ReviewParts';
import { moveRoving } from '@/features/learning/quizzes/items/roving';

const rowBase = 'flex min-h-11 w-full items-start gap-3 rounded-xl border px-4 py-3 text-left text-sm leading-6 transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:cursor-not-allowed disabled:opacity-60';

/** One pill row of a radiogroup. Arrow keys move the choice, as in a native group. */
export function ChoiceRow({ label, letter, checked, focusable, disabled, onSelect }: {
  label: string; letter: string; checked: boolean; focusable: boolean; disabled: boolean; onSelect: (next?: number) => void;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const next = moveRoving(event, '[role="radio"]');
    if (next !== null) onSelect(next);
  };
  return <button type="button" role="radio" aria-checked={checked} tabIndex={focusable ? 0 : -1} disabled={disabled}
    onClick={() => onSelect()} onKeyDown={onKeyDown}
    className={`${rowBase} ${checked ? 'border-accent bg-accent/5 text-text-primary ring-1 ring-accent/30' : 'border-border text-text-secondary hover:bg-background'}`}>
    <span aria-hidden="true" className={`grid h-6 w-6 shrink-0 place-items-center rounded-full border text-xs font-semibold ${checked ? 'border-accent bg-accent text-accent-fg' : 'border-border text-text-muted'}`}>{letter}</span>
    <span className="min-w-0 pt-px">{label}</span>
  </button>;
}

export function ChoiceGroup({ options, selected, disabled, promptId, onChange }: {
  options: string[]; selected: number | undefined; disabled: boolean; promptId: string; onChange: (index: number) => void;
}) {
  return <div role="radiogroup" aria-labelledby={promptId} data-roving className="space-y-2.5">
    {options.map((option, index) => <ChoiceRow key={`${index}-${option}`} label={option} letter={optionLetter(index)}
      checked={selected === index} focusable={selected === undefined ? index === 0 : selected === index}
      disabled={disabled} onSelect={(next) => onChange(next ?? index)} />)}
  </div>;
}

export function ChoiceEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  return <ChoiceGroup options={item.options} selected={draft.selectedIndices[0]} disabled={disabled} promptId={promptId}
    onChange={(index) => onChange({ selectedIndices: [index] })} />;
}

function choiceWords(chosen: boolean, key: boolean) {
  if (chosen && key) return <PartMark state="right">Your answer · correct</PartMark>;
  if (chosen) return <PartMark state="wrong">Your answer · not correct</PartMark>;
  if (key) return <PartMark state="right">Correct answer</PartMark>;
  return null;
}

/** Every option with what you chose and the key, then why: your pick's flaw first. */
export function ChoiceReview({ item, draft, reveal }: ReviewProps) {
  const chosen = draft.selectedIndices[0];
  const key = reveal.correctIndices[0];
  const wrong = chosen !== undefined && !reveal.correctIndices.includes(chosen);
  const others = item.options.map((option, index) => ({ option, index, why: reveal.distractorRationales[index] ?? '' }))
    .filter(({ index, why }) => why.trim() && index !== chosen && !reveal.correctIndices.includes(index));
  return <div>
    <ul className="space-y-2">{item.options.map((option, index) => <li key={`${index}-${option}`}
      className={`flex flex-wrap items-start gap-3 rounded-xl border px-4 py-2.5 text-sm leading-6 ${reveal.correctIndices.includes(index) ? 'border-emerald-500/30 bg-emerald-500/5' : index === chosen ? 'border-rose-500/30 bg-rose-500/5' : 'border-border'}`}>
      <span aria-hidden="true" className="grid h-6 w-6 shrink-0 place-items-center rounded-full border border-border text-xs font-semibold text-text-muted">{optionLetter(index)}</span>
      <span className="min-w-0 flex-1 text-text-secondary">{option}</span>
      {choiceWords(index === chosen, reveal.correctIndices.includes(index))}
    </li>)}</ul>
    {wrong && key !== undefined && <ReasonLine>You chose {optionLetter(chosen)}; the key is {optionLetter(key)}.{reveal.distractorRationales[chosen]?.trim() ? ` ${reveal.distractorRationales[chosen]}` : ''}</ReasonLine>}
    {chosen === undefined && <ReasonLine>No answer given. The key is {key !== undefined ? optionLetter(key) : 'shown above'}.</ReasonLine>}
    <KeyExplanation text={reveal.explanation} />
    {others.length > 0 && <details className="mt-3 text-sm" open={wrong}>
      <summary className="cursor-pointer text-xs font-semibold text-text-secondary">Why the others fail</summary>
      <ul className="mt-2 space-y-1.5 text-text-secondary">{others.map(({ index, why }) => <li key={index}><span className="font-semibold text-text-primary">{optionLetter(index)}.</span> {why}</li>)}</ul>
    </details>}
  </div>;
}
