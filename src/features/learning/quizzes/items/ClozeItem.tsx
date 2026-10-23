import { Fragment } from 'react';

import { blankStates } from '@/features/learning/quizzes/items/answerMatch';
import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark } from '@/features/learning/quizzes/items/ReviewParts';

const BLANK = /\{\{\s*(\d+)\s*\}\}/g;

type Segment = { text: string } | { blank: number };

/** "The {{1}} binds the {{2}}" → text and blank segments, blanks numbered from 0. */
export function clozeSegments(prompt: string): Segment[] {
  const segments: Segment[] = [];
  let last = 0;
  for (const match of prompt.matchAll(BLANK)) {
    if (match.index > last) segments.push({ text: prompt.slice(last, match.index) });
    segments.push({ blank: Number(match[1]) - 1 });
    last = match.index + match[0].length;
  }
  if (last < prompt.length) segments.push({ text: prompt.slice(last) });
  return segments;
}

function blankCount(item: EditorProps['item'], segments: Segment[]) {
  return Math.max(item.blankCount, ...segments.map((segment) => ('blank' in segment ? segment.blank + 1 : 0)));
}

/** The prompt with its blanks inline, each a labelled input ("Blank 1 of 2"). */
export function ClozeEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  const segments = clozeSegments(item.prompt);
  const total = blankCount(item, segments);
  const setBlank = (index: number, value: string) => {
    const next = Array.from({ length: total }, (_, i) => draft.blankResponses[i] ?? '');
    next[index] = value;
    onChange({ blankResponses: next });
  };
  const input = (index: number) => <input key={`blank-${index}`} type="text" autoComplete="off" aria-label={`Blank ${index + 1} of ${total}`}
    value={draft.blankResponses[index] ?? ''} disabled={disabled} onChange={(event) => setBlank(index, event.target.value)}
    className="mx-1 inline-block min-h-11 w-40 max-w-full rounded-lg border-0 border-b-2 border-accent/50 bg-accent/5 px-2 align-baseline font-sans text-base text-text-primary outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/30 disabled:opacity-60" />;
  const inline = segments.some((segment) => 'blank' in segment);
  return <div>
    <p id={promptId} className="max-w-3xl font-serif text-[1.35rem] leading-[2.6rem] text-text-primary">
      {segments.map((segment, index) => ('blank' in segment ? input(segment.blank) : <Fragment key={index}>{segment.text}</Fragment>))}
    </p>
    {!inline && <div className="mt-4 flex flex-wrap gap-3">{Array.from({ length: total }, (_, index) => input(index))}</div>}
    <p className="mt-3 text-xs text-text-muted">Fill each blank. Capitals, spacing and punctuation don&apos;t matter.</p>
  </div>;
}

/** Each blank: what you wrote, the accepted answer, and a verdict when the score allows one. */
export function ClozeReview({ item, draft, reveal, result }: ReviewProps) {
  const share = result && result.gradeStatus !== 'pending' && result.pointsPossible > 0 ? result.pointsEarned / result.pointsPossible : null;
  const states = blankStates(draft.blankResponses, reveal.blanks, share);
  return <div>
    <p className="text-sm leading-7 text-text-secondary">{clozeSegments(item.prompt).map((segment, index) => ('blank' in segment
      ? <span key={index} className="mx-0.5 rounded bg-background px-1.5 font-semibold text-text-primary">[{segment.blank + 1}]</span>
      : <Fragment key={index}>{segment.text}</Fragment>))}</p>
    <ul className="mt-3 space-y-2">{reveal.blanks.map((key, index) => {
      const yours = draft.blankResponses[index]?.trim();
      return <li key={index} className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-xl border border-border px-4 py-2.5 text-sm">
        <span className="font-semibold text-text-primary">Blank {index + 1}</span>
        {states[index] === 'right' && <PartMark state="right">Correct</PartMark>}
        {states[index] === 'wrong' && <PartMark state="wrong">Not correct</PartMark>}
        <span className="text-text-secondary">You wrote {yours ? `“${yours}”` : 'nothing'}</span>
        {states[index] !== 'right' && <span className="text-text-secondary">· accepted: <span className="font-medium text-text-primary">{key}</span></span>}
      </li>;
    })}</ul>
    <KeyExplanation text={reveal.explanation} />
  </div>;
}
