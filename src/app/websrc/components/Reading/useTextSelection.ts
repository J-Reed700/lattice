import { useEffect, useRef, useState } from 'react';

export interface TextSelectionState {
  text: string;
  /** Viewport-relative rect of the selection, for positioning the toolbar. */
  rect: DOMRect | null;
}

const EMPTY: TextSelectionState = { text: '', rect: null };

/**
 * Tracks the text selection inside `containerRef` (BRIEF rank 6).
 *
 * `rect` is the raw viewport-relative `getBoundingClientRect()` — no scroll
 * offsets added — because the toolbar renders into `document.body` with
 * `position: fixed`. Selection is cleared on any scroll (capture phase) so a
 * stale toolbar never floats over the wrong text.
 */
export function useTextSelection(
  containerRef: React.RefObject<HTMLElement | null>
): TextSelectionState {
  const [selection, setSelection] = useState<TextSelectionState>(EMPTY);
  const frameRef = useRef<number | null>(null);

  useEffect(() => {
    const read = () => {
      frameRef.current = null;
      const container = containerRef.current;
      if (!container) {
        setSelection((current) => (current.text ? EMPTY : current));
        return;
      }

      const active = window.getSelection();
      if (!active || active.isCollapsed || active.rangeCount === 0) {
        setSelection((current) => (current.text ? EMPTY : current));
        return;
      }

      const range = active.getRangeAt(0);
      if (!container.contains(range.commonAncestorContainer)) {
        setSelection((current) => (current.text ? EMPTY : current));
        return;
      }

      const text = active.toString().trim();
      const rect = range.getBoundingClientRect();
      if (!text || rect.width === 0 || rect.height === 0) {
        setSelection((current) => (current.text ? EMPTY : current));
        return;
      }

      setSelection({ text, rect });
    };

    const schedule = () => {
      if (frameRef.current !== null) return;
      frameRef.current = requestAnimationFrame(read);
    };

    const clear = () => {
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
      setSelection((current) => (current.text ? EMPTY : current));
    };

    document.addEventListener('selectionchange', schedule);
    // Capture phase: the scrolling element is usually an inner container, and a
    // bubbling listener on `document` would never see its scroll event.
    document.addEventListener('scroll', clear, true);

    return () => {
      document.removeEventListener('selectionchange', schedule);
      document.removeEventListener('scroll', clear, true);
      if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
    };
  }, [containerRef]);

  return selection;
}
