import { useCallback, useContext } from 'react';

import { QueryClientContext } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import type { JournalNotify } from '@/features/journal/hooks/useJournalNotice';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import type { AppNavigate } from '@/hooks/useAppNavigate';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { clearDraft, conversationDraftKey } from '@/shared/drafts/drafts';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';

/** Starts, renames and deletes the conversations filed under a journal. */
export function useJournalEntryActions(options: {
  requestedJournalSpaceId: string | null;
  navigate: AppNavigate;
  notify: JournalNotify;
  saveNow: () => Promise<boolean>;
  reloadEntries: () => Promise<void>;
  removeEntry: (conversationId: string) => void;
}) {
  const api = useApi();
  const queryClient = useContext(QueryClientContext);
  const { requestedJournalSpaceId, navigate, notify, saveNow, reloadEntries, removeEntry } = options;
  const createConversationViaStore = useConversationsStore((s) => s.createConversation);
  const renameConversationViaStore = useConversationsStore((s) => s.renameConversation);
  const deleteConversationViaStore = useConversationsStore((s) => s.deleteConversation);

  // `spaceId` is the space whose documents the chat will search. A journal is
  // not a space, so nothing here implies one: the sidebar asks, and the palette
  // — which cannot — starts in General rather than in whatever the Chat sidebar
  // last had selected, a choice made on another screen for another reason. The
  // chat names its space above the composer either way.
  const handleNewEntry = useCallback(async (spaceId: string = GENERAL_SPACE_ID) => {
    if (!requestedJournalSpaceId) return;
    try {
      const newConversationId = await createConversationViaStore(
        createDefaultConversationTitle(),
        spaceId,
      );
      const linkResult = await api.addConversationToJournal({
        journalSpaceId: requestedJournalSpaceId,
        conversationId: newConversationId,
      });
      if (!linkResult.ok) {
        notify('error', `Failed to attach entry to journal: ${linkResult.error}`);
        return;
      }
      // A conversation is something you have in Chat. Filing an empty one here
      // and stopping left a row that could be neither read nor written in.
      if (!(await saveNow())) return;
      navigate(`/chat?conversationId=${encodeURIComponent(newConversationId)}`);
    } catch (error) {
      notify(
        'error',
        error instanceof Error ? error.message : 'Failed to start a conversation.',
      );
    }
  }, [api, createConversationViaStore, navigate, notify, requestedJournalSpaceId, saveNow]);

  const handleRenameEntry = useCallback(
    async (entryId: string, title: string) => {
      if (renameConversationViaStore) {
        const renamed = await renameConversationViaStore(entryId, title);
        if (!renamed) {
          notify('error', 'Failed to rename entry.');
          return;
        }
      } else {
        const result = await api.renameConversation(entryId, title);
        if (!result.ok) {
          notify('error', `Failed to rename entry: ${result.error}`);
          return;
        }
      }
      if (queryClient) {
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: conversationKeys.all }),
          queryClient.invalidateQueries({ queryKey: ['dashboard'] }),
        ]);
      }
      await reloadEntries();
    },
    [api, notify, queryClient, reloadEntries, renameConversationViaStore],
  );

  const handleDeleteEntry = useCallback(
    async (entryId: string) => {
      if (deleteConversationViaStore) {
        const deleted = await deleteConversationViaStore(entryId);
        if (!deleted) {
          notify('error', 'Failed to delete entry.');
          return;
        }
      } else {
        const result = await api.deleteConversation(entryId);
        if (!result.ok) {
          notify('error', `Failed to delete entry: ${result.error}`);
          return;
        }
      }
      clearDraft(conversationDraftKey(entryId));
      if (queryClient) {
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: conversationKeys.all }),
          queryClient.invalidateQueries({ queryKey: ['dashboard'] }),
        ]);
      }
      removeEntry(entryId);
      notify('success', 'Entry deleted.');
    },
    [api, deleteConversationViaStore, notify, queryClient, removeEntry],
  );

  return { handleNewEntry, handleRenameEntry, handleDeleteEntry };
}
