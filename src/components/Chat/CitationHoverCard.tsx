import { FileText } from 'lucide-react';
import { createPortal } from 'react-dom';


import type { SourceWithMetadata } from '@/types/conversation';
import { sanitizeFileName } from '@/utils/sanitize';

export interface CitationHover {
  number: number;
  /** Viewport rect of the chip under the pointer. */
  rect: DOMRect;
}

interface CitationHoverCardProps {
  hover: CitationHover | null;
  source: SourceWithMetadata | null;
  location?: string | null;
  provenanceLabel?: string;
}

const CARD_WIDTH = 340;
const GAP = 8;

/**
 * The passage behind a citation chip, shown while the pointer rests on it.
 * Pointer-only and non-interactive by design: the chip itself is the control,
 * and clicking it opens the source in the reading pane.
 */
export function CitationHoverCard({ hover, source, location, provenanceLabel }: CitationHoverCardProps) {
  if (!hover || !source) return null;

  const passage = (source.excerpt || source.content || '').trim();
  const left = Math.min(
    Math.max(12, hover.rect.left + hover.rect.width / 2 - CARD_WIDTH / 2),
    window.innerWidth - CARD_WIDTH - 12,
  );
  // Above the chip unless that would leave the window.
  const placeAbove = hover.rect.top > 220;
  const style = placeAbove
    ? { left, bottom: window.innerHeight - hover.rect.top + GAP, width: CARD_WIDTH }
    : { left, top: hover.rect.bottom + GAP, width: CARD_WIDTH };
  const meta = [provenanceLabel, location].filter(Boolean).join(' · ');

  return createPortal(
    <div
      role="tooltip"
      style={style}
      className="pointer-events-none fixed z-[60] rounded-lg bg-surface-overlay p-3.5 shadow-lg animate-in fade-in-0 duration-fast"
    >
      <div className="flex items-start gap-2.5">
        <span className="mt-px flex h-5 min-w-5 items-center justify-center rounded-[5px] bg-accent-muted px-1 text-[11px] font-semibold tabular-nums text-accent">
          {hover.number}
        </span>
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-1.5 truncate text-ui font-medium text-text-primary">
            <FileText className="h-3.5 w-3.5 shrink-0 text-text-tertiary" strokeWidth={1.6} />
            <span className="truncate">{sanitizeFileName(source.fileName)}</span>
          </p>
          {meta ? <p className="mt-0.5 truncate text-xs text-text-muted">{meta}</p> : null}
        </div>
      </div>
      {passage ? (
        <p className="mt-2.5 line-clamp-5 border-l-2 border-accent/50 pl-2.5 font-serif text-[13.5px] leading-relaxed text-text-secondary">
          {passage}
        </p>
      ) : null}
      <p className="mt-2.5 text-[11px] text-text-muted">Click to open the source</p>
    </div>,
    document.body,
  );
}
