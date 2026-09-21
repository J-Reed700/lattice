import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';

import { measureCaret } from './caretCoordinates';
import { SUGGEST_PANEL_WIDTH } from './ComposerSuggest';
import { readTrigger } from './suggestTrigger';

import type { CaretPoint } from './caretCoordinates';
import type { SuggestItem } from './ComposerSuggest';
import type { SuggestTrigger } from './suggestTrigger';

/**
 * The keyboard and the caret behind the composer's suggestion popup.
 *
 * The composer owns its text; this owns only "is a list open, which row is
 * lit, and where does it point". `handleKeyDown` says whether it took the key,
 * so the composer's own Enter never fires while a list is open, and a key that
 * an IME is still composing is never either an accept or a send.
 */

interface UseComposerSuggestOptions {
  value: string;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  /** The rows for a trigger. Called during render, so it must stay cheap. */
  resolveItems: (_trigger: SuggestTrigger) => SuggestItem[];
  /** Keep an empty list open — a document search that has not answered yet. */
  isBusy?: (_trigger: SuggestTrigger) => boolean;
  onAccept: (_item: SuggestItem, _trigger: SuggestTrigger) => void;
}

export interface ComposerSuggestController {
  trigger: SuggestTrigger | null;
  items: SuggestItem[];
  isOpen: boolean;
  activeIndex: number;
  point: CaretPoint | null;
  setActiveIndex: (_index: number) => void;
  /** Tell the hook where the caret went — every handler that can move it. */
  syncCaret: (_target?: HTMLTextAreaElement | null) => void;
  /** A floating list over a composer nobody is typing in is just litter. */
  setFocused: (_focused: boolean) => void;
  accept: (_index: number) => void;
  close: () => void;
  /** True when the popup took the key, so the composer must leave it alone. */
  handleKeyDown: (_event: React.KeyboardEvent<HTMLTextAreaElement>) => boolean;
}

const NO_ITEMS: SuggestItem[] = [];

export function useComposerSuggest({
  value,
  textareaRef,
  resolveItems,
  isBusy,
  onAccept,
}: UseComposerSuggestOptions): ComposerSuggestController {
  const [caret, setCaret] = useState(0);
  const [activeIndex, setActiveIndex] = useState(0);
  const [dismissedAt, setDismissedAt] = useState<number | null>(null);
  const [point, setPoint] = useState<CaretPoint | null>(null);
  const [isFocused, setIsFocused] = useState(true);

  const trigger = readTrigger(value, caret);
  const items = trigger ? resolveItems(trigger) : NO_ITEMS;
  const busy = trigger ? Boolean(isBusy?.(trigger)) : false;
  const isOpen =
    trigger !== null &&
    isFocused &&
    dismissedAt !== trigger.start &&
    (items.length > 0 || busy);

  const boundedIndex = items.length === 0 ? 0 : Math.min(activeIndex, items.length - 1);

  const acceptRef = useRef(onAccept);
  acceptRef.current = onAccept;
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const triggerRef = useRef(trigger);
  triggerRef.current = trigger;

  const syncCaret = useCallback(
    (target?: HTMLTextAreaElement | null) => {
      const element = target ?? textareaRef.current;
      if (!element) return;
      // A selection is not a caret typing a command.
      setCaret(
        element.selectionStart === element.selectionEnd ? element.selectionStart : -1
      );
    },
    [textareaRef]
  );

  const close = useCallback(() => {
    setDismissedAt(triggerRef.current?.start ?? -1);
  }, []);

  // Escape suppresses the list for the token it was pressed in; a new token
  // anywhere gets a fresh chance.
  const triggerStart = trigger?.start ?? null;
  useEffect(() => {
    if (triggerStart === null) setDismissedAt(null);
  }, [triggerStart]);

  // A different query is a different list, so the lit row goes back to the top.
  const triggerKey = trigger ? `${trigger.kind}:${trigger.query}` : null;
  useEffect(() => {
    setActiveIndex(0);
  }, [triggerKey]);

  useLayoutEffect(() => {
    const element = textareaRef.current;
    if (!isOpen || !element || triggerStart === null) {
      setPoint(null);
      return;
    }
    const measured = measureCaret(element, triggerStart);
    // A caret near the right edge would push the panel off the composer.
    const maxLeft = Math.max(0, element.clientWidth - SUGGEST_PANEL_WIDTH);
    setPoint({ left: Math.min(measured.left, maxLeft), top: measured.top });
  }, [isOpen, triggerStart, value, textareaRef]);

  const accept = useCallback((index: number) => {
    const item = itemsRef.current[index];
    const current = triggerRef.current;
    if (!item || !current) return;
    setDismissedAt(current.start);
    acceptRef.current(item, current);
  }, []);

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
      // Enter that commits an IME candidate is not an accept and not a send.
      if (event.nativeEvent.isComposing) return false;
      if (!isOpen) return false;

      const count = itemsRef.current.length;
      switch (event.key) {
        case 'ArrowDown':
          event.preventDefault();
          if (count > 0) setActiveIndex((index) => (Math.min(index, count - 1) + 1) % count);
          return true;
        case 'ArrowUp':
          event.preventDefault();
          if (count > 0) {
            setActiveIndex((index) => (Math.min(index, count - 1) + count - 1) % count);
          }
          return true;
        case 'Escape':
          event.preventDefault();
          close();
          return true;
        case 'Tab':
          if (event.shiftKey) return false;
          event.preventDefault();
          accept(boundedIndex);
          return true;
        case 'Enter':
          // Shift+Enter is a new line in the composer, open list or not.
          if (event.shiftKey) return false;
          event.preventDefault();
          accept(boundedIndex);
          // Taken either way: an open list never lets Enter send.
          return true;
        default:
          return false;
      }
    },
    [accept, boundedIndex, close, isOpen]
  );

  return {
    trigger,
    items,
    isOpen,
    activeIndex: boundedIndex,
    point,
    setActiveIndex,
    syncCaret,
    setFocused: setIsFocused,
    accept,
    close,
    handleKeyDown,
  };
}
