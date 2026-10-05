import { useEffect, useRef, type RefObject } from 'react';

/**
 * Put the caret back in the composer when a turn ends.
 *
 * The composer stays typeable during a turn, but the Stop button takes focus
 * and then unmounts, which drops focus to `body`. Only then does the composer
 * take it back: a reader who moved on to another field keeps their place.
 */
export function useRefocusAfterTurn(
  isSending: boolean,
  textareaRef: RefObject<HTMLTextAreaElement | null>
): void {
  const wasSending = useRef(isSending);
  useEffect(() => {
    const turnEnded = wasSending.current && !isSending;
    wasSending.current = isSending;
    if (!turnEnded) return;
    const focused = document.activeElement;
    if (focused === null || focused === document.body) {
      textareaRef.current?.focus();
    }
  }, [isSending, textareaRef]);
}
