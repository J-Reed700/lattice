import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { VirtualizedMessageListHandle } from '@/features/chat/components/VirtualizedMessageList';

/**
 * Following the thread: which message is in view, jumping to one or to the
 * latest, which messages just arrived, and keeping the newest in view while
 * the reader is at the bottom.
 */
export function useThreadScroll<TMessage>({
  activeConversationId,
  messages,
  getMessageKey,
  prefersReducedMotion,
}: {
  activeConversationId: string | null;
  messages: TMessage[];
  getMessageKey: (message: TMessage) => string;
  prefersReducedMotion: boolean | null;
}) {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const messageListRef = useRef<VirtualizedMessageListHandle>(null);
  const previousConversationIdRef = useRef<string | null>(null);
  const previousMessageCountRef = useRef(0);
  const shouldAutoScrollRef = useRef(true);
  const [visibleMessage, setVisibleMessage] = useState<{ conversationId: string | null; index: number }>({ conversationId: null, index: 0 });
  const knownMessageIdsRef = useRef<{ conversationId: string | null; ids: Set<string> }>({
    conversationId: null,
    ids: new Set(),
  });

  const handleVisibleMessage = useCallback((index: number) => {
    setVisibleMessage({ conversationId: activeConversationId, index });
  }, [activeConversationId]);

  const navigateToMessage = useCallback((index: number) => {
    // Jump to the beginning even when the answer is several screens tall.
    // Disable following first so a streaming update cannot pull us back down.
    shouldAutoScrollRef.current = false;
    messageListRef.current?.scrollToIndex(index, { align: 'start', behavior: 'auto' });
  }, []);

  const navigateToLatest = useCallback(() => {
    shouldAutoScrollRef.current = true;
    messageListRef.current?.scrollToIndex(messages.length - 1, { align: 'end', behavior: 'auto' });
  }, [messages.length]);

  const freshMessageKeys = useMemo(() => {
    const fresh = new Set<string>();
    const tracker = knownMessageIdsRef.current;
    if (tracker.conversationId !== activeConversationId) {
      tracker.conversationId = activeConversationId ?? null;
      tracker.ids = new Set(messages.map(getMessageKey));
      return fresh;
    }
    for (const message of messages) {
      const key = getMessageKey(message);
      if (!tracker.ids.has(key)) {
        fresh.add(key);
        tracker.ids.add(key);
      }
    }
    return fresh;
  }, [activeConversationId, getMessageKey, messages]);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;

    const previousConversationId = previousConversationIdRef.current;
    const isConversationChange = previousConversationId !== activeConversationId;
    const isInitialMessageLoad = previousMessageCountRef.current === 0;
    previousConversationIdRef.current = activeConversationId ?? null;
    previousMessageCountRef.current = messages.length;

    if (isConversationChange) {
      shouldAutoScrollRef.current = true;
    } else if (!shouldAutoScrollRef.current) {
      return;
    }

    // The global `prefers-reduced-motion` rule in index.css covers CSS
    // transitions, not programmatic scrolling: this ride has to be opted out
    // of here or it happens on every message.
    messageListRef.current?.scrollToIndex(messages.length - 1, {
      align: 'end',
      // The conversation can arrive before its messages. Treat that first
      // batch as a jump too; smooth scrolling through estimated long rows
      // can stop partway down the history and compete with a navigation click.
      behavior: isConversationChange || isInitialMessageLoad || prefersReducedMotion ? 'auto' : 'smooth',
    });
  }, [messages, activeConversationId, prefersReducedMotion]);

  return {
    scrollContainerRef,
    messageListRef,
    shouldAutoScrollRef,
    visibleMessage,
    handleVisibleMessage,
    navigateToMessage,
    navigateToLatest,
    freshMessageKeys,
  };
}
