import { useCallback, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { unwrap } from '@/features/chat/api/conversationQueryData';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { VaultAPI } from '@/lib/api';
import type { ConversationTangentDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error));

/**
 * Promote, delete or rename a tangent. Shared by the open tangent and its row
 * in the list, so both confirm, report and refresh the list the same way.
 */
export function useTangentActions(drafts: Map<string, string>) {
  const queryClient = useQueryClient();
  const deleteConversation = useConversationsStore(state => state.deleteConversation);
  const renameConversation = useConversationsStore(state => state.renameConversation);
  const selectConversation = useConversationsStore(state => state.selectConversation);
  const pending = useRef(false);
  const [mutating, setMutating] = useState(false);

  const busy = useCallback((tangent: ConversationTangentDto) =>
    pending.current || conversationUiStore.getState().inFlightGenerations.has(tangent.conversationId), []);

  const run = useCallback(async (tangent: ConversationTangentDto, work: () => Promise<boolean>): Promise<boolean> => {
    if (busy(tangent)) return false;
    pending.current = true;
    setMutating(true);
    try {
      return await work();
    } finally {
      pending.current = false;
      setMutating(false);
      void queryClient.invalidateQueries({ queryKey: conversationKeys.tangents(tangent.parentConversationId) });
    }
  }, [busy, queryClient]);

  const finish = useCallback(async (tangent: ConversationTangentDto, action: 'promote' | 'delete'): Promise<boolean> => {
    if (busy(tangent) || (action === 'delete' && !window.confirm('Delete this tangent and its messages?'))) return false;
    const id = tangent.conversationId;
    return run(tangent, async () => {
      try {
        if (action === 'promote') {
          const conversation = unwrap(await VaultAPI.promoteConversationTangent(id));
          queryClient.setQueryData(conversationKeys.detail(id), conversation);
          await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
          toast.success('Tangent is now a conversation', { action: { label: 'Open conversation', onClick: () => { void selectConversation(id); } } });
        } else {
          // The shared delete action also cancels listeners and evicts caches.
          if (!await deleteConversation(id)) throw new Error(conversationUiStore.getState().error ?? 'Please try again.');
        }
        queryClient.setQueryData<ConversationTangentDto[]>(conversationKeys.tangents(tangent.parentConversationId), current => current?.filter(item => item.conversationId !== id));
        drafts.delete(id);
        return true;
      } catch (error) {
        toast.error(action === 'promote' ? "Couldn't make this a conversation" : "Couldn't delete the tangent", { message: errorText(error) });
        return false;
      }
    });
  }, [busy, deleteConversation, drafts, queryClient, run, selectConversation]);

  /** Tangents are conversations, so this is the sidebar's rename. */
  const rename = useCallback(async (tangent: ConversationTangentDto, title: string): Promise<boolean> => {
    const next = title.trim();
    if (!next || next === tangent.title) return true;
    return run(tangent, async () => {
      if (!await renameConversation(tangent.conversationId, next)) {
        toast.error("Couldn't rename the tangent", { message: conversationUiStore.getState().error ?? 'Please try again.' });
        return false;
      }
      queryClient.setQueryData<ConversationTangentDto[]>(conversationKeys.tangents(tangent.parentConversationId), current =>
        current?.map(item => (item.conversationId === tangent.conversationId ? { ...item, title: next } : item)));
      return true;
    });
  }, [queryClient, renameConversation, run]);

  return { finish, rename, mutating };
}
