import { Fragment, useMemo, type ReactNode } from 'react';

import {
  articleBlocks,
  type ArticleBlock,
  type ArticleListItem,
  type TextRange,
} from '@/features/reading/model/articleBlocks';
import { blocksWithin, inlineRuns, type InlineRun } from '@/features/reading/model/inlineMarkdown';
import { toast } from '@/stores/toastStore';
import { whyExternalUrlDidNotOpen } from '@/utils/openExternalUrl';

/** A passage to mark, as an offset range into the article's text. */
export interface ArticlePassage {
  start: number;
  end: number;
  title?: string;
}

export interface ArticleTextProps {
  text: string;
  /** The blocks to draw; defaults to the blocks of `text`. */
  blocks?: ArticleBlock[];
  passages?: ArticlePassage[];
  /** The passage drawn lit: an index into `passages`. */
  activeIndex?: number;
  /**
   * Called with the first mark drawn for each passage, in document order: where
   * the reader scrolls to. A passage cut by formatting or by a block break is
   * several marks, and only the first is the place it begins.
   */
  onMarkElement?: (index: number, element: HTMLElement | null) => void;
  /** For previews: whole blocks until this many visible characters (at least one block). */
  maxChars?: number;
}

async function openLink(href: string): Promise<void> {
  const failure = await whyExternalUrlDidNotOpen(href);
  if (failure !== null) toast.error("Couldn't open link", { message: failure });
}

const CELL_ALIGN = {
  left: 'text-left',
  center: 'text-center',
  right: 'text-right',
} as const;

const HEADING_BASE = 'mb-3 mt-7 font-serif font-semibold leading-snug text-[hsl(var(--text-primary))] first:mt-0';
const BODY = 'font-serif text-[15px] leading-7 text-[hsl(var(--text-primary))]';

/**
 * Saved page text, drawn as the Markdown it is.
 *
 * Only how characters display changes: syntax characters are hidden and the
 * rest of the text keeps its offsets, so the passages marked by offset in the
 * raw text land on the same words. Each marked stretch is one `<mark>` with the
 * formatting inside it, so a passage across bold text is a single mark.
 */
export function ArticleText({ text, blocks, passages, activeIndex = -1, onMarkElement, maxChars }: ArticleTextProps) {
  const parsed = useMemo(() => blocks ?? articleBlocks(text), [blocks, text]);
  const shown = useMemo(
    () => (maxChars === undefined ? parsed : blocksWithin(text, parsed, maxChars).blocks),
    [maxChars, parsed, text],
  );
  const runCache = useMemo(() => ({ text, runs: new Map<string, InlineRun[]>() }), [text]).runs;
  const ordered = useMemo(
    () =>
      (passages ?? [])
        .map((passage, index) => ({ passage, index }))
        .sort((a, b) => a.passage.start - b.passage.start || a.index - b.index),
    [passages],
  );

  const registered = new Set<number>();

  const runsOf = (range: TextRange): InlineRun[] => {
    const key = `${range.start}:${range.end}`;
    let runs = runCache.get(key);
    if (!runs) {
      runs = inlineRuns(text, range.start, range.end);
      runCache.set(key, runs);
    }
    return runs;
  };

  const styled = (shownText: string, run: InlineRun, key: string): ReactNode => {
    let node: ReactNode = shownText;
    if (run.code) {
      node = <code className="rounded bg-[hsl(var(--text-primary)/0.07)] px-1 font-mono text-[0.9em]">{node}</code>;
    }
    if (run.image) node = <span className="italic text-[hsl(var(--text-muted))]">{node}</span>;
    if (run.strike) node = <del>{node}</del>;
    if (run.em) node = <em>{node}</em>;
    if (run.strong) node = <strong>{node}</strong>;
    const href = run.href;
    if (href) {
      node = (
        <a
          href={href}
          title={href}
          className="underline underline-offset-2"
          onClick={(event) => {
            event.preventDefault();
            void openLink(href);
          }}
        >
          {node}
        </a>
      );
    }
    return <Fragment key={key}>{node}</Fragment>;
  };

  /** The runs of one segment: a run that is not what its source says shows whole where it starts. */
  const segmentNodes = (runs: InlineRun[], from: number, to: number, key: string): ReactNode[] => {
    const nodes: ReactNode[] = [];
    for (const run of runs) {
      let shownText: string;
      if (run.atomic === true) {
        if (run.start < from || run.start >= to) continue;
        shownText = run.text;
      } else {
        const a = Math.max(run.start, from);
        const b = Math.min(run.end, to);
        if (b <= a) continue;
        shownText = text.slice(a, b);
      }
      nodes.push(styled(shownText, run, `${key}-${run.start}`));
    }
    return nodes;
  };

  /** One content range, split at the passages that overlap it. */
  const renderRange = (range: TextRange, inline: boolean): ReactNode => {
    if (range.end <= range.start) return null;
    const runs = inline ? runsOf(range) : [{ start: range.start, end: range.end, text: text.slice(range.start, range.end) }];
    const key = `r${range.start}`;
    const marked = ordered.filter(({ passage }) => passage.end > range.start && passage.start < range.end);
    if (marked.length === 0) return segmentNodes(runs, range.start, range.end, key);

    const out: ReactNode[] = [];
    let cursor = range.start;
    const plain = (from: number, to: number) => {
      out.push(...segmentNodes(runs, from, to, `${key}-p${from}`));
    };
    for (const { passage, index } of marked) {
      const from = Math.max(passage.start, cursor);
      const to = Math.min(passage.end, range.end);
      if (to <= from) continue;
      if (from > cursor) plain(cursor, from);
      const inside = segmentNodes(runs, from, to, `${key}-m${from}`);
      if (inside.length > 0) {
        const first = !registered.has(index);
        registered.add(index);
        out.push(
          <mark
            key={`${key}-mark${from}`}
            ref={first && onMarkElement ? (element) => onMarkElement(index, element) : undefined}
            className={index === activeIndex ? 'source-reader-passage is-lit' : 'source-reader-passage'}
            title={passage.title}
          >
            {inside}
          </mark>,
        );
      }
      cursor = to;
    }
    if (cursor < range.end) plain(cursor, range.end);
    return out;
  };

  /** Items by depth, as nested lists. Depth never jumps more than a level at once. */
  const renderList = (items: ArticleListItem[], blockStart: number): ReactNode => {
    const depths: number[] = [];
    items.forEach((item, position) => {
      depths.push(Math.min(item.depth, position === 0 ? 0 : depths[position - 1]! + 1));
    });

    const level = (from: number, depth: number): [ReactNode, number] => {
      const first = items[from]!;
      const isOrdered = first.ordered;
      const entries: ReactNode[] = [];
      let at = from;
      while (at < items.length && depths[at]! >= depth && (depths[at]! > depth || items[at]!.ordered === isOrdered)) {
        const item = items[at]!;
        at += 1;
        const children: ReactNode[] = [];
        while (at < items.length && depths[at]! > depth) {
          const [child, next] = level(at, depth + 1);
          children.push(child);
          at = next;
        }
        entries.push(
          <li key={item.start} className="pl-1">
            {renderRange(item, true)}
            {children}
          </li>,
        );
      }
      const className = `${depth === 0 ? 'mb-4' : 'mt-1'} ${isOrdered ? 'list-decimal' : 'list-disc'} pl-6 ${BODY}`;
      const node = isOrdered ? (
        <ol key={first.start} className={className} start={first.number !== undefined && first.number !== 1 ? first.number : undefined}>
          {entries}
        </ol>
      ) : (
        <ul key={first.start} className={className}>
          {entries}
        </ul>
      );
      return [node, at];
    };

    const lists: ReactNode[] = [];
    let at = 0;
    while (at < items.length) {
      const [node, next] = level(at, 0);
      lists.push(node);
      at = next;
    }
    return <Fragment key={blockStart}>{lists}</Fragment>;
  };

  const renderBlock = (block: ArticleBlock): ReactNode => {
    switch (block.kind) {
      case 'heading': {
        const major = (block.level ?? 2) <= 2;
        const Heading = major ? 'h4' : 'h5';
        return (
          <Heading key={block.start} className={`${HEADING_BASE} ${major ? 'text-lg' : 'text-base'}`}>
            {renderRange(block, true)}
          </Heading>
        );
      }
      case 'code':
        return (
          <pre
            key={block.start}
            className="mb-4 overflow-x-auto rounded-md bg-[hsl(var(--text-primary)/0.05)] px-4 py-3 font-mono text-xs leading-5 text-[hsl(var(--text-primary))]"
          >
            <code>{renderRange(block, false)}</code>
          </pre>
        );
      case 'list':
        return renderList(block.items ?? [], block.start);
      case 'quote':
        return (
          <blockquote
            key={block.start}
            className="mb-4 border-l-2 border-[hsl(var(--text-muted)/0.5)] pl-4 font-serif text-[15px] leading-7 text-[hsl(var(--text-secondary))]"
          >
            {(block.lines ?? []).map((line, position) => (
              <span key={line.start}>
                {position > 0 ? ' ' : null}
                {renderRange(line, true)}
              </span>
            ))}
          </blockquote>
        );
      case 'table': {
        const rows = block.rows ?? [];
        const [header = [], ...body] = rows;
        const columns = Math.max(0, ...rows.map((row) => row.length));
        const align = (column: number) => {
          const value = block.align?.[column];
          return value ? CELL_ALIGN[value] : 'text-left';
        };
        const cells = (row: TextRange[], Cell: 'th' | 'td') =>
          Array.from({ length: columns }, (_, column) => {
            const cell = row[column];
            return (
              <Cell
                key={column}
                className={`border border-border-subtle px-2.5 py-1.5 align-top ${align(column)} ${
                  Cell === 'th' ? 'bg-[hsl(var(--text-primary)/0.04)] font-semibold' : ''
                }`}
                {...(Cell === 'th' ? { scope: 'col' } : {})}
              >
                {cell ? renderRange(cell, true) : null}
              </Cell>
            );
          });
        return (
          <div key={block.start} className="mb-4 overflow-x-auto">
            <table className="w-full border-collapse font-serif text-[13px] leading-5 text-[hsl(var(--text-primary))]">
              <thead>
                <tr>{cells(header, 'th')}</tr>
              </thead>
              <tbody>
                {body.map((row, position) => (
                  <tr key={position}>{cells(row, 'td')}</tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      }
      case 'rule':
        return <hr key={block.start} className="my-6 border-border-subtle" />;
      default:
        return (
          <p key={block.start} className={`mb-4 ${BODY}`}>
            {renderRange(block, true)}
          </p>
        );
    }
  };

  return <>{shown.map(renderBlock)}</>;
}
