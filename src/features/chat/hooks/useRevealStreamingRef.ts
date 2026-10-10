import { useEffect, useRef } from 'react';

import { findCodeRefs } from '@/components/TiptapEditor/extensions/codeRefMarks';
import { revealInFolder } from '@/features/chat/model/folderThreadHost';

/** How often a streaming answer is checked for a new reference. */
const CHECK_MS = 400;

/**
 * While a folder thread's answer streams, show the newest line reference it has
 * written. Only a new reference moves the view; one already shown stays put
 * so the reader can scroll away from it.
 *
 * A timer rather than a debounce: tokens arrive faster than any debounce
 * window, so a debounced check would only ever run once the answer is done.
 */
export function useRevealStreamingRef(content: string, active: boolean): void {
  const contentRef = useRef(content);
  const shownRef = useRef<string | null>(null);

  useEffect(() => {
    contentRef.current = content;
  }, [content]);

  useEffect(() => {
    if (!active) return;
    const check = (complete: boolean) => {
      // A reference that runs to the end of the text may still be growing
      // (`src/a.rs:1` on its way to `:12`), so mid-stream it waits for the
      // next character; the last check, once the answer is done, takes it.
      const refs = findCodeRefs(contentRef.current, { complete });
      const newest = refs[refs.length - 1];
      if (!newest) return;
      const key = `${newest.path}:${newest.startLine}-${newest.endLine}`;
      if (key === shownRef.current) return;
      shownRef.current = key;
      revealInFolder(newest.path, { startLine: newest.startLine, endLine: newest.endLine });
    };
    const timer = window.setInterval(() => check(false), CHECK_MS);
    return () => {
      window.clearInterval(timer);
      // The last tokens can land between two checks.
      check(true);
    };
  }, [active]);
}
