import { useEffect, useRef } from 'react';

import {
  approximateScrollRatio,
  buildNeedles,
  normalizeForMatch,
} from './passageLocator';
import './reading.css';

import type { PassageLocator, PassageMatchTier } from '../../types/conversation';

/**
 * Highlights a cited passage inside rendered document content.
 *
 * Highlights at **block** granularity — it adds a class to the paragraphs the
 * passage spans rather than splicing markup into them. Anything finer would be
 * invasive inside `TiptapViewer`'s ProseMirror DOM and would be
 * lost on its next `setContent`; a class on an existing element survives
 * everything short of that, and is idempotent.
 *
 * Reports which of three things happened, so the rail can say so:
 * - `exact` — the chunk text was found.
 * - `approximate` — it was not, and we scrolled to a position derived from the
 *   chunk ordinal. The file has probably changed since it was indexed.
 * - `none` — we have nothing to go on and did not move the viewport.
 */

const BLOCK_SELECTOR =
  'p, li, h1, h2, h3, h4, h5, h6, blockquote, pre, td, .lattice-line';

const BLOCK_CLASS = 'lattice-passage-block';

interface TextSpan {
  node: Text;
  start: number;
  end: number;
}

/** Build a normalized haystack plus the spans that map back into the DOM. */
function collectText(container: HTMLElement): { haystack: string; spans: TextSpan[] } {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  const spans: TextSpan[] = [];
  let haystack = '';

  let node = walker.nextNode() as Text | null;
  while (node) {
    const normalized = normalizeForMatch(node.data);
    if (normalized) {
      const start = haystack.length === 0 ? 0 : haystack.length + 1;
      haystack = haystack.length === 0 ? normalized : `${haystack} ${normalized}`;
      spans.push({ node, start, end: start + normalized.length });
    }
    node = walker.nextNode() as Text | null;
  }

  return { haystack, spans };
}

function nearestBlock(node: Node, container: HTMLElement): HTMLElement | null {
  const start = node.nodeType === Node.ELEMENT_NODE ? (node as HTMLElement) : node.parentElement;
  const block = start?.closest(BLOCK_SELECTOR) as HTMLElement | null;
  if (block && container.contains(block)) return block;
  return start && container.contains(start) ? start : null;
}

/** True when the reader has asked the system for less motion. */
function prefersReducedMotion(): boolean {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
}

function nearestScrollable(element: HTMLElement): HTMLElement | null {
  let current: HTMLElement | null = element;
  while (current) {
    if (current.scrollHeight > current.clientHeight + 8) return current;
    current = current.parentElement;
  }
  return null;
}

export interface PassageHighlighterProps {
  locator?: PassageLocator | null;
  /** Reported once per locate attempt so the rail can say what happened. */
  onMatch?: (_tier: PassageMatchTier) => void;
  className?: string;
  children: React.ReactNode;
}

export function PassageHighlighter({
  locator,
  onMatch,
  className,
  children,
}: PassageHighlighterProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const onMatchRef = useRef(onMatch);
  onMatchRef.current = onMatch;

  const locatorText = locator?.text ?? '';
  const chunkIndex = locator?.chunkIndex;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const report = (tier: PassageMatchTier) => onMatchRef.current?.(tier);

    if (!locatorText.trim()) {
      report('none');
      return;
    }

    const markedBlocks: HTMLElement[] = [];
    let cancelled = false;

    // Content arrives asynchronously in every viewer (markdown parse, syntax
    // highlighting, fetch), so locate after a frame rather than racing it.
    const frame = requestAnimationFrame(() => {
      if (cancelled) return;

      const { haystack, spans } = collectText(container);
      let hitStart = -1;
      let hitEnd = -1;

      for (const needle of [normalizeForMatch(locatorText), ...buildNeedles(locatorText)]) {
        const index = haystack.indexOf(needle);
        if (index >= 0) {
          hitStart = index;
          hitEnd = index + needle.length;
          break;
        }
      }

      if (hitStart < 0) {
        if (typeof chunkIndex === 'number') {
          const scroller = nearestScrollable(container);
          if (scroller) {
            scroller.scrollTop = approximateScrollRatio(chunkIndex) * scroller.scrollHeight;
          }
          report('approximate');
        } else {
          report('none');
        }
        return;
      }

      const startSpan = spans.find((span) => span.end > hitStart);
      const endSpan = [...spans].reverse().find((span) => span.start < hitEnd);
      const startBlock = startSpan ? nearestBlock(startSpan.node, container) : null;
      const endBlock = endSpan ? nearestBlock(endSpan.node, container) : null;

      if (!startBlock) {
        report('none');
        return;
      }

      const allBlocks = Array.from(container.querySelectorAll<HTMLElement>(BLOCK_SELECTOR));
      const firstIndex = allBlocks.indexOf(startBlock);
      const lastIndex = endBlock ? allBlocks.indexOf(endBlock) : firstIndex;
      const range =
        firstIndex >= 0 && lastIndex >= firstIndex
          ? allBlocks.slice(firstIndex, lastIndex + 1)
          : [startBlock];

      for (const block of range) {
        block.classList.add(BLOCK_CLASS);
        markedBlocks.push(block);
      }

      // Guarded: jsdom (and any non-layout environment) has no scrollIntoView,
      // and failing to scroll must not lose the highlight. A reader who asked
      // for less motion still gets taken to the passage, just without the ride.
      range[0]?.scrollIntoView?.({
        block: 'center',
        behavior: prefersReducedMotion() ? 'auto' : 'smooth',
      });
      report('exact');
    });

    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
      for (const block of markedBlocks) block.classList.remove(BLOCK_CLASS);

    };
  }, [locatorText, chunkIndex]);

  return (
    <div ref={containerRef} className={className}>
      {children}
    </div>
  );
}
