import { createPortal } from 'react-dom';

import { CitationCard } from '@/features/reading/citations/CitationCard';
import { anchoredEntry } from '@/features/reading/citations/source';
import type { SourceWithMetadata } from '@/types/conversation';
import { sanitizeFileName } from '@/utils/sanitize';

export interface CitationHover {
  number: number;
  /** The chip's place among its source's chips (`data-cite-at`), when it has one. */
  occurrence: number | null;
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
 * The passage behind a citation chip, shown while the pointer rests on it: the
 * citation card, which says how the chip uses the source when it knows.
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

  const entry = anchoredEntry(source, hover.occurrence);

  return createPortal(
    <div
      role="tooltip"
      style={style}
      className="pointer-events-none fixed z-60 rounded-lg bg-surface-overlay p-3.5 shadow-lg animate-in fade-in-0 duration-fast"
    >
      <CitationCard
        number={hover.number}
        title={sanitizeFileName(source.fileName)}
        relation={entry?.relation}
        excerpts={entry?.anchors.map((anchor) => anchor.exact).filter(Boolean) ?? []}
        passage={passage}
        footer={
          <>
            {meta ? <p className="mt-2 truncate text-xs text-text-muted">{meta}</p> : null}
            <p className="mt-2.5 text-[11px] text-text-muted">Click to open the source</p>
          </>
        }
      />
    </div>,
    document.body,
  );
}
