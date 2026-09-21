import { CircleCheck, CircleHelp, CircleX } from 'lucide-react';
import { createPortal } from 'react-dom';

import type { ClaimVerdict, SourceWithMetadata } from '@/types/conversation';
import { sanitizeFileName } from '@/utils/sanitize';


export interface ClaimHover {
  /** Index into the message's `claimVerdicts`. */
  index: number;
  /** Viewport rect of the line of the sentence under the pointer. */
  rect: DOMRect;
  /** Right edge of the answer's text column; the card stays left of it. */
  maxRight: number;
}

interface ClaimHoverCardProps {
  hover: ClaimHover | null;
  verdict: ClaimVerdict | null;
  citationMap: Map<number, SourceWithMetadata>;
}

const CARD_WIDTH = 360;
const GAP = 8;

function headline(verdict: ClaimVerdict) {
  if (verdict.verdict === 'supported') {
    return { icon: CircleCheck, tone: 'text-[hsl(var(--success-fg))]', text: 'Backed by the source' };
  }
  if (verdict.verdict === 'contradicted') {
    return { icon: CircleX, tone: 'text-[hsl(var(--danger-fg))]', text: 'The source says otherwise' };
  }
  return {
    icon: CircleHelp,
    tone: 'text-[hsl(var(--warning-fg))]',
    text: verdict.citationIds.length > 0 ? 'Not found in the cited passage' : 'No source cited for this',
  };
}

/**
 * Why a sentence of the answer is or is not trusted, shown while the pointer
 * rests on it: the verdict, how it was reached, and the words it rests on.
 *
 * How it was reached is said out loud because the two checks are not equally
 * strong: shared wording can echo a source while stating the opposite of it,
 * and a reader deciding whether to quote a figure should know which one ran.
 */
export function ClaimHoverCard({ hover, verdict, citationMap }: ClaimHoverCardProps) {
  if (!hover || !verdict) return null;

  const { icon: Icon, tone, text } = headline(verdict);
  // Inside the text column, so it never covers the margin note it lights up.
  const rightLimit = Math.min(hover.maxRight, window.innerWidth - 12);
  const left = Math.max(12, Math.min(hover.rect.left, rightLimit - CARD_WIDTH));
  const placeAbove = hover.rect.top > 240;
  const style = placeAbove
    ? { left, bottom: window.innerHeight - hover.rect.top + GAP, width: CARD_WIDTH }
    : { left, top: hover.rect.bottom + GAP, width: CARD_WIDTH };
  const cited = verdict.citationIds
    .map((id) => ({ id, source: citationMap.get(id) }))
    .filter((entry): entry is { id: number; source: SourceWithMetadata } => Boolean(entry.source));

  return createPortal(
    <div
      role="tooltip"
      style={style}
      className="pointer-events-none fixed z-[60] rounded-lg bg-surface-overlay p-3.5 shadow-lg animate-in fade-in-0 duration-fast"
    >
      <p className={`flex items-center gap-1.5 text-ui font-medium ${tone}`}>
        <Icon className="h-3.5 w-3.5 shrink-0" strokeWidth={1.8} aria-hidden="true" />
        {text}
      </p>
      <p className="mt-0.5 pl-5 text-xs text-text-muted">
        {verdict.method === 'judge'
          ? 'The model read the passage against this sentence'
          : 'Matched on shared wording, not read for meaning'}
      </p>
      {verdict.evidenceQuote ? (
        <p className="mt-2.5 line-clamp-5 border-l-2 border-accent/50 pl-2.5 font-serif text-[13.5px] leading-relaxed text-text-secondary">
          &ldquo;{verdict.evidenceQuote}&rdquo;
        </p>
      ) : null}
      {cited.length > 0 ? (
        <ul className="mt-2.5 space-y-1">
          {cited.map(({ id, source }) => (
            <li key={id} className="flex items-center gap-2 text-xs text-text-secondary">
              <span className="flex h-4 min-w-4 items-center justify-center rounded-[4px] bg-accent-muted px-1 text-[10px] font-semibold tabular-nums text-accent">
                {id}
              </span>
              <span className="truncate">{sanitizeFileName(source.fileName)}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>,
    document.body,
  );
}
