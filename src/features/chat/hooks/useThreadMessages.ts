import { useCallback, useMemo } from 'react';

import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';

/**
 * The open conversation's messages, persisted and optimistic, with the keys
 * and raw content the thread and its panels read them by.
 */
export function useThreadMessages(
  activeConversationId: string | null,
  conversations: ConversationsState['conversations'],
  optimisticMessages: ConversationsState['optimisticMessages']
) {
  const messages = useMemo(() => {
    if (!activeConversationId) return [];

    const realMessages =
      conversations.find((conversation) => conversation.id === activeConversationId)?.messages ?? [];
    const optimisticForConversation = Array.from(optimisticMessages.values()).filter(
      (message) =>
        message.conversationId === activeConversationId || message.conversationId === 'temp'
    );
    return [...realMessages, ...optimisticForConversation];
  }, [activeConversationId, conversations, optimisticMessages]);

  /**
   * Raw content of the loaded messages, for resolving memory evidence spans.
   *
   * The memory panel needs the stored text, not the rendered markdown: its
   * spans are UTF-8 byte offsets into what the backend holds.
   */
  const messageContentById = useMemo(() => {
    const byId = new Map<string, string>();
    for (const message of messages) {
      if ('id' in message && typeof message.content === 'string') {
        byId.set(message.id, message.content);
      }
    }
    return byId;
  }, [messages]);

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
