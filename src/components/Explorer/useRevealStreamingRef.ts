import { useEffect, useRef } from 'react';

import { findCodeRefs } from '@/components/TiptapEditor/extensions/codeRefMarks';
import { useExplorerStore } from '@/stores/explorerStore';

/** How often a streaming answer is checked for a new reference. */
const CHECK_MS = 400;

/**
 * While an Explorer answer streams, show the newest line reference it has
 * written. Only a new reference moves the view; one already shown stays put
 * so the reader can scroll away from it.
 *
 * A timer rather than a debounce: tokens arrive faster than any debounce
 * window, so a debounced check would only ever run once the answer is done.
 */
export function useRevealStreamingRef(content: string, active: boolean): void {
  const reveal = useExplorerStore((state) => state.reveal);
  const contentRef = useRef(content);
  const shownRef = useRef<string | null>(null);

  useEffect(() => {
    contentRef.current = content;
  }, [content]);

  useEffect(() => {
    if (!active) return;
    const check = () => {
      // Only closed backticks count, so a reference still being typed out
      // (`src/a.rs:1` on its way to `:12`) is never shown half-written.
      const refs = findCodeRefs(contentRef.current);
      const newest = refs[refs.length - 1];
      if (!newest) return;
      const key = `${newest.path}:${newest.startLine}-${newest.endLine}`;
      if (key === shownRef.current) return;
      shownRef.current = key;
      reveal(newest.path, { startLine: newest.startLine, endLine: newest.endLine });
    };
    const timer = window.setInterval(check, CHECK_MS);
    return () => {
      window.clearInterval(timer);
      // The last tokens can land between two checks.
      check();
    };
  }, [active, reveal]);
}
