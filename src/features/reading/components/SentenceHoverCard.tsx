import type { ReactNode } from 'react';

import { createPortal } from 'react-dom';

export interface ClaimHover {
  /** Index into the text's checked sentences. */
  index: number;
  /** Viewport rect of the line of the sentence under the pointer. */
  rect: DOMRect;
  /** Right edge of the text column; the card stays left of it. */
  maxRight: number;
}

const CARD_WIDTH = 360;
const GAP = 8;

/** The positioned card a sentence's hover content sits in. */
export function SentenceHoverCard({ hover, children }: { hover: ClaimHover | null; children: ReactNode }) {
  if (!hover) return null;

  // Inside the text column, so it never covers the margin note it lights up.
  const rightLimit = Math.min(hover.maxRight, window.innerWidth - 12);
  const left = Math.max(12, Math.min(hover.rect.left, rightLimit - CARD_WIDTH));
  const placeAbove = hover.rect.top > 240;
  const style = placeAbove
    ? { left, bottom: window.innerHeight - hover.rect.top + GAP, width: CARD_WIDTH }
    : { left, top: hover.rect.bottom + GAP, width: CARD_WIDTH };

  return createPortal(
    <div
      role="tooltip"
      style={style}
      className="pointer-events-none fixed z-60 rounded-lg bg-surface-overlay p-3.5 shadow-lg animate-in fade-in-0 duration-fast"
    >
      {children}
    </div>,
    document.body,
  );
}
