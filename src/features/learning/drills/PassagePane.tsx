import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';

import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';
import type { CodeReviewDrillDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';

import { pickSentence, sentencesOf } from './passageText';

const within = (range: ExplorerLineRange | null, line: number) =>
  range !== null && line >= range.startLine && line <= range.endLine;

/**
 * The explanation to review, as one paragraph of prose: each sentence can be picked, alone or, with
 * shift, as a run from the first one picked. Arrow keys move between sentences and, with shift, extend the run.
 * While the learner writes a fix, an editable copy of the explanation, one sentence to a line.
 */
export function PassagePane({ drill, writingFix, selection, onSelectLines, highlight, highlightNonce, fixEdits, onFixEdit }: {
  drill: CodeReviewDrillDto;
  writingFix: boolean;
  /** The sentences picked, by the line of the explanation each is on. */
  selection: ExplorerLineRange | null;
  onSelectLines: (range: ExplorerLineRange | null) => void;
  /** Sentences to scroll to, whenever `highlightNonce` changes. */
  highlight: ExplorerLineRange | null;
  highlightNonce: number | null;
  /** The text of each file the learner changed in their fix, by path. */
  fixEdits: Readonly<Record<string, string>>;
  onFixEdit: (path: string, text: string) => void;
}) {
  const file = drill.files[0];
  const text = file?.text ?? '';
  const all = useMemo(() => sentencesOf(text), [text]);
  const lines = useMemo(() => all.map((sentence) => sentence.line), [all]);
  const [current, setCurrent] = useState<number | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const reached = current !== null && lines.includes(current) ? current : (lines[0] ?? null);

  useEffect(() => {
    if (highlightNonce === null || !highlight) return;
    root.current?.querySelector(`[data-line="${highlight.startLine}"]`)?.scrollIntoView?.({ block: 'nearest' });
  }, [highlight, highlightNonce]);

  if (!file) return null;

  const move = (event: KeyboardEvent<HTMLButtonElement>, line: number) => {
    const step = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;
    if (!step && event.key !== 'Home' && event.key !== 'End') return;
    event.preventDefault();
    const index = lines.indexOf(line);
    const place = event.key === 'Home' ? 0 : event.key === 'End' ? lines.length - 1 : Math.min(lines.length - 1, Math.max(0, index + step));
    const target = lines[place];
    if (target === undefined) return;
    if (event.shiftKey) onSelectLines(pickSentence(selection ?? { startLine: line, endLine: line }, target, true));
    root.current?.querySelector<HTMLButtonElement>(`[data-line="${target}"]`)?.focus();
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-2xl border border-border bg-surface">
      <div className="flex items-center justify-between gap-2 border-b border-border bg-background px-4 py-2.5">
        <h2 className="text-xs font-medium text-text-secondary">The explanation</h2>
        {writingFix && <span className="shrink-0 text-xs font-medium text-text-secondary">Your fix: an editable copy</span>}
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {writingFix ? (
          <div className="flex h-full flex-col gap-2 p-4">
            <p className="text-xs leading-5 text-text-secondary">
              Keep one sentence to a line: your rewrite is read against the explanation line by line.
            </p>
            <textarea
              aria-label="Edit the explanation"
              value={fixEdits[file.path] ?? file.text}
              onChange={(event) => onFixEdit(file.path, event.target.value)}
              className="min-h-[20rem] w-full flex-1 resize-none rounded-xl border border-border bg-background px-4 py-3 text-[15px] leading-7 text-text-primary outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/15"
            />
          </div>
        ) : (
          <div
            ref={root}
            role="group"
            aria-label="The explanation, sentence by sentence"
            className="mx-auto max-w-prose space-y-5 px-6 py-6 text-[15px] leading-8 text-text-primary"
          >
            {all.length > 0 && (
              <p>
                {all.map((sentence) => {
                  const picked = within(selection, sentence.line);
                  return (
                    <span key={sentence.line}>
                      <button
                        type="button"
                        data-line={sentence.line}
                        aria-label={`Sentence ${sentence.line}: ${sentence.text}`}
                        aria-pressed={picked}
                        tabIndex={sentence.line === reached ? 0 : -1}
                        onFocus={() => setCurrent(sentence.line)}
                        onClick={(event) => onSelectLines(pickSentence(selection, sentence.line, event.shiftKey))}
                        onKeyDown={(event) => move(event, sentence.line)}
                        className={cn(
                          'group relative inline rounded px-0.5 text-inherit [box-decoration-break:clone] [-webkit-box-decoration-break:clone] transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent',
                          picked ? 'bg-accent-muted' : 'hover:bg-surface-hover',
                          within(highlight, sentence.line) && 'ring-2 ring-accent/50',
                        )}
                      >
                        <sup
                          aria-hidden="true"
                          className={cn(
                            'pointer-events-none absolute -top-2 left-0 text-[10px] font-semibold leading-none tabular-nums text-accent group-hover:opacity-100 group-focus-visible:opacity-100',
                            picked ? 'opacity-100' : 'opacity-0',
                          )}
                        >
                          {sentence.line}
                        </sup>
                        {sentence.text}
                      </button>{' '}
                    </span>
                  );
                })}
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
