import { TiptapViewer } from '@/components/TiptapEditor';
import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';
import { KeyExplanation, PartMark, partAt, PartWhy } from '@/features/learning/quizzes/items/ReviewParts';

const BLANK = /\{\{\s*(\d+)\s*\}\}/g;
const MATH = /\$[^$]+\$/;

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

/** Prompt text between blanks. Maths goes through the viewer so KaTeX renders it, laid out inline. */
export function ClozeText({ text }: { text: string }) {
  if (!MATH.test(text)) return <>{text}</>;
  return <>
    {/^\s/.test(text) && ' '}
    <span className="[&_div]:inline [&_p]:inline">
      <TiptapViewer content={text.trim()} />
    </span>
    {/\s$/.test(text) && ' '}
  </>;
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
    <div id={promptId} className="max-w-3xl font-serif text-[1.35rem] leading-[2.6rem] text-text-primary">
      {segments.map((segment, index) => ('blank' in segment ? input(segment.blank) : <ClozeText key={index} text={segment.text} />))}
    </div>
    {!inline && <div className="mt-4 flex flex-wrap gap-3">{Array.from({ length: total }, (_, index) => input(index))}</div>}
    <p className="mt-3 text-xs text-text-muted">Fill each blank. Capitals, spacing and punctuation don&apos;t matter.</p>
  </div>;
}

/** Each blank: what you wrote and the accepted answer, with the grader's verdict and reason once graded. */
export function ClozeReview({ item, draft, reveal, result }: ReviewProps) {
  return <div>
    <div className="text-sm leading-7 text-text-secondary">{clozeSegments(item.prompt).map((segment, index) => ('blank' in segment
      ? <span key={index} className="mx-0.5 rounded bg-background px-1.5 font-semibold text-text-primary">[{segment.blank + 1}]</span>
      : <ClozeText key={index} text={segment.text} />))}</div>
    <ul className="mt-3 space-y-2">{reveal.blanks.map((key, index) => {
      const yours = draft.blankResponses[index]?.trim();
      const part = partAt(result, index);
      return <li key={index} className="rounded-xl border border-border px-4 py-2.5 text-sm">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span className="font-semibold text-text-primary">Blank {index + 1}</span>
          {part && <PartMark state={part.correct ? 'right' : 'wrong'}>{part.correct ? 'Correct' : 'Not correct'}</PartMark>}
          <span className="text-text-secondary">You wrote {yours ? `“${yours}”` : 'nothing'}</span>
          {!part?.correct && <span className="text-text-secondary">· accepted: <span className="font-medium text-text-primary">{key}</span></span>}
        </div>
        {part && !part.correct && <PartWhy text={part.detail} />}
      </li>;
    })}</ul>
    <KeyExplanation text={reveal.explanation} />
  </div>;
}
