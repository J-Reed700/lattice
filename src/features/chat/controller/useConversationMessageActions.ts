import { useCallback } from 'react';

import { useApi } from '@/contexts/ApiContext';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { saveBookmark, removeBookmark } from '@/features/references/api/queries';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ConversationMessage } from '@/types/conversation';

import { restoreMissingItem, setUiError, type ConversationActionsDependencies } from './conversationActionHelpers';
import { toConversationMessage } from '../api/conversationQueryData';

/** Changing a conversation's messages: references, truncation, deletion, failed sends, the draft and errors. */
export function useConversationMessageActions({
  queryClient,
  lifecycle,
  invalidateLists,
}: Pick<ConversationActionsDependencies, 'queryClient' | 'lifecycle'> & {
  invalidateLists: () => Promise<void>;
}) {
  const api = useApi();

  const bookmarkMessage = useCallback(async (
    conversationId: string,
    messageId: string,
    title?: string | null,
    note?: string | null
  ) => {
    try {
      await saveBookmark(queryClient, { conversationId, messageId, title, note }, api);
      return true;
    } catch (error) {
      setUiError(error);
      return false;
    }
  }, [api, queryClient]);

  const unbookmarkMessage = useCallback(async (conversationId: string, messageId: string) => {
    try {
      await removeBookmark(queryClient, { conversationId, messageId }, api);
      return true;
    } catch (error) {
      setUiError(error);
      return false;
    }
  }, [api, queryClient]);

  /** Truncate persisted history after a message and refresh the conversation list. */
  const truncateAfter = useCallback(async (
    conversationId: string,
    messageId: string,
    inclusive = false
  ): Promise<boolean> => {
    let cancelled = false;
    const unregisterLifecycle = lifecycle.register(conversationId, () => { cancelled = true; });
    try {
      const result = await api.truncateConversationAfter(conversationId, messageId, inclusive);
      // A truncate request can outlive a concurrent conversation deletion.
      // Never recreate its just-removed messages cache with a late response.
      if (cancelled) return false;
      if (!result.ok) {
        setUiError(result.error);
        return false;
      }
      queryClient.setQueryData(
        conversationKeys.messages(conversationId),
        result.data.messages.map(toConversationMessage)
      );
      await invalidateLists();
      return true;
    } finally {
      unregisterLifecycle();
    }
  }, [api, invalidateLists, lifecycle, queryClient]);

  const deleteMessage = useCallback(async (conversationId: string, messageId: string) => {
    const key = conversationKeys.messages(conversationId);
    const previous = queryClient.getQueryData<ConversationMessage[]>(key);
    if (!previous?.some(message => message.id === messageId)) {
      setUiError('Message not found in the selected conversation.');
      return;
    }
    let cancelled = false;
    const unregisterLifecycle = lifecycle.register(conversationId, () => { cancelled = true; });
    queryClient.setQueryData(key, previous.filter(message => message.id !== messageId));
    try {
      const result = await api.deleteConversationMessage({ conversationId, messageId });
      if (cancelled) return;
      if (!result.ok) {
        queryClient.setQueryData<ConversationMessage[]>(key, current =>
          restoreMissingItem(current, previous, messageId));
        setUiError(result.error);
        return;
      }
      await Promise.all([
        invalidateLists(),
        queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks }),
      ]);
    } finally {
      unregisterLifecycle();
    }
  }, [api, invalidateLists, lifecycle, queryClient]);

  const dismissFailedMessage = useCallback((tempId: string) => {
    conversationUiStore.setState(current => {
      if (current.optimisticMessages.get(tempId)?.status !== 'failed') return current;
      const optimisticMessages = new Map(current.optimisticMessages);
      optimisticMessages.delete(tempId);
      return { optimisticMessages };
    });
  }, []);

  /** Pure UI state: the text the composer should adopt on its next render. */
  const setComposerDraft = useCallback(
    (draft: string | null) => conversationUiStore.setState({ composerDraft: draft }),
    []
  );

  const clearError = useCallback(() => conversationUiStore.setState({ error: null }), []);

  return {
    bookmarkMessage, unbookmarkMessage, truncateAfter, deleteMessage,
    dismissFailedMessage, setComposerDraft, clearError,
  };
}
