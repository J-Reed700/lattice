import { useCallback, useMemo } from 'react';

import { persistedRequestIds } from '@/features/chat/controller/optimisticMessages';
import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';
import type { ConversationMessage, OptimisticMessage } from '@/types/conversation';

const EMPTY_REAL_MESSAGES: ConversationMessage[] = [];

/**
 * The open conversation's messages, persisted and optimistic, with the keys
 * and raw content the thread and its panels read them by.
 */
export function useThreadMessages(
  activeConversationId: string | null,
  conversations: ConversationsState['conversations'],
  optimisticMessages: ConversationsState['optimisticMessages'] | readonly OptimisticMessage[]
) {
  const realMessages = useMemo(() => {
    if (!activeConversationId) return EMPTY_REAL_MESSAGES;
    return (
      conversations.find((conversation) => conversation.id === activeConversationId)?.messages ??
      EMPTY_REAL_MESSAGES
    );
  }, [activeConversationId, conversations]);

  const messages = useMemo(() => {
    if (!activeConversationId) return [];

    const persistedIds = persistedRequestIds(realMessages);
    const candidates = Array.isArray(optimisticMessages)
      ? optimisticMessages
      : Array.from((optimisticMessages as ConversationsState['optimisticMessages']).values());
    const optimisticForConversation = candidates.filter(
      (message) =>
        (message.conversationId === activeConversationId || message.conversationId === 'temp') &&
        !(message.role === 'user' && message.requestId && persistedIds.has(message.requestId))
    );
    return [...realMessages, ...optimisticForConversation];
  }, [activeConversationId, realMessages, optimisticMessages]);

  /**
   * Raw content of the loaded messages, for resolving memory evidence spans.
   *
   * The memory panel needs the stored text, not the rendered markdown: its
   * spans are UTF-8 byte offsets into what the backend holds.
   */
  const messageContentById = useMemo(() => {
    const byId = new Map<string, string>();
    for (const message of realMessages) {
      if ('id' in message && typeof message.content === 'string') {
        byId.set(message.id, message.content);
      }
    }
    return byId;
  }, [realMessages]);

  const getMessageKey = useCallback(
    (message: typeof messages[number]): string =>
      'tempId' in message ? message.tempId : message.id,
    []
  );
  const getPersistedMessageId = useCallback(
    (message: typeof messages[number]): string | null => 'id' in message ? message.id : null,
    []
  );

  return { messages, messageContentById, getMessageKey, getPersistedMessageId };
}
