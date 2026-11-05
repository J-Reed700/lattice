import { useEffect, useId, useMemo, useRef } from 'react';

import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';
import { sentencesOf } from '@/features/learning/drills/passageText';
import type { ContrastFocus } from '@/features/learning/drills/reveal/CodeContrast';
import { plainText } from '@/features/learning/drills/reveal/plainText';
import { cleanLinesOf, type RankedIssue } from '@/features/learning/drills/reveal/revealPresentation';
import { eyebrow, sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import { quotePieces } from '@/features/learning/drills/reveal/sourceQuotes';
import type { CodeReviewFileDto, RevealedSourceDto } from '@/lib/bindings';

const within = (range: ExplorerLineRange | null, line: number) =>
  range !== null && line >= range.startLine && line <= range.endLine;

function Explanation({ title, text, marked, focused, nonce, tone, word }: {
  title: string;
  text: string;
  /** The sentences to mark, by line. */
  marked: readonly ExplorerLineRange[];
  /** The sentences of the issue the learner is looking at. */
  focused: ExplorerLineRange | null;
  nonce: number | null;
  tone: string;
  /** What a reader of the screen is told a marked sentence is. */
  word: string;
}) {
  const titleId = useId();
  const root = useRef<HTMLDivElement>(null);
  const all = useMemo(() => sentencesOf(text), [text]);
  useEffect(() => {
    if (nonce === null || !focused) return;
    root.current?.querySelector(`[data-line="${focused.startLine}"]`)?.scrollIntoView?.({ block: 'nearest' });
  }, [focused, nonce]);
  return (
    <div role="group" aria-labelledby={titleId} className="min-w-0 overflow-hidden rounded-2xl border border-border bg-bg">
      <div className="border-b border-border px-4 py-2">
        <h3 id={titleId} className={eyebrow}>{title}</h3>
      </div>
      <div ref={root} className="max-h-[34rem] space-y-4 overflow-auto px-5 py-4 text-[15px] leading-8 text-text-primary">
        {all.length === 0 && <p className="text-sm text-text-muted">This version has no explanation.</p>}
        {all.length > 0 && (
          <p>
            {all.map((sentence) => {
              const isMarked = marked.some((range) => within(range, sentence.line));
              return (
                <span key={sentence.line} data-line={sentence.line}>
                  {isMarked ? (
                    <mark className={`rounded px-0.5 text-text-primary ${tone} ${within(focused, sentence.line) ? 'ring-2 ring-accent' : ''}`}>
                      <span className="sr-only">{word}: </span>
                      {sentence.text}
                    </mark>
                  ) : sentence.text}
                  {' '}
                </span>
              );
            })}
          </p>
        )}
      </div>
    </div>
  );
}

/**
 * The contrasting-cases step for an explanation: the one the learner reviewed with its planted sentences marked,
 * beside the same explanation without the errors, and under them what the source says, with the words that settle
 * each issue marked in it.
 */
export function PassageContrast({ reviewed, clean, ranked, cleanFiles, source, focus }: {
  reviewed: CodeReviewFileDto | undefined;
  clean: CodeReviewFileDto | undefined;
  ranked: readonly RankedIssue[];
  cleanFiles: readonly CodeReviewFileDto[];
  source: RevealedSourceDto | null;
  focus: ContrastFocus | null;
}) {
  const headingId = useId();
  const sourceId = useId();
  const planted = ranked.map((entry) => ({ startLine: entry.issue.span.start, endLine: entry.issue.span.end }));
  const restored = ranked.flatMap((entry) => {
    const range = cleanLinesOf(entry.issue, cleanFiles);
    return range ? [range] : [];
  });
  const quotes = ranked.flatMap((entry) => (entry.issue.sourceQuote ? [plainText(entry.issue.sourceQuote)] : []));
  const text = source ? plainText(source.text) : '';
  const pieces = source ? quotePieces(text, quotes, focus?.quote ? plainText(focus.quote) : null) : [];
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <div>
        <h2 id={headingId} className={sectionHeading}>The explanation you reviewed · The explanation without the errors</h2>
        <p className="mt-1 text-sm text-text-secondary">Choose an issue’s sentences above to see them here, in both versions.</p>
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <Explanation
          title="The explanation you reviewed"
          text={reviewed?.text ?? ''}
          marked={planted}
          focused={focus?.reviewed ?? null}
          nonce={focus?.nonce ?? null}
          tone="bg-danger-muted"
          word="Planted error"
        />
        <Explanation
          title="The explanation without the errors"
          text={clean?.text ?? ''}
          marked={restored}
          focused={focus?.clean ?? null}
          nonce={focus?.clean ? focus.nonce : null}
          tone="bg-success-muted"
          word="Without the error"
        />
      </div>
      {source && (
        <div role="group" aria-labelledby={sourceId} className="min-w-0 overflow-hidden rounded-2xl border border-border bg-bg">
          <div className="flex flex-wrap items-baseline justify-between gap-x-3 border-b border-border px-4 py-2">
            <h3 id={sourceId} className={eyebrow}>What the source says</h3>
            <span className="text-xs text-text-muted">{source.title}</span>
          </div>
          <p className="max-h-[28rem] overflow-auto whitespace-pre-wrap px-5 py-4 text-[15px] leading-8 text-text-primary">
            {pieces.length === 0
              ? text
              : pieces.map((piece, index) => (piece.quoted
                ? <mark key={index} className={`rounded px-0.5 text-text-primary ${piece.focused ? 'bg-accent-muted ring-2 ring-accent' : 'bg-warning-muted'}`}>{piece.text}</mark>
                : <span key={index}>{piece.text}</span>))}
          </p>
        </div>
      )}
    </section>
  );
}
