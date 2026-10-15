import { useCallback } from 'react';

import type { JournalNotify } from '@/features/journal/hooks/useJournalNotice';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';

import type { NavigateFunction } from 'react-router';

/** Starts, renames and deletes the conversations filed under a journal. */
export function useJournalEntryActions(options: {
  requestedJournalSpaceId: string | null;
  navigate: NavigateFunction;
  notify: JournalNotify;
  saveNow: () => Promise<boolean>;
  reloadEntries: () => Promise<void>;
  removeEntry: (conversationId: string) => void;
}) {
  const { requestedJournalSpaceId, navigate, notify, saveNow, reloadEntries, removeEntry } = options;
  const createConversationViaStore = useConversationsStore((s) => s.createConversation);

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
      const linkResult = await VaultAPI.addConversationToJournal({
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
  }, [createConversationViaStore, navigate, notify, requestedJournalSpaceId, saveNow]);

  const handleRenameEntry = useCallback(
    async (entryId: string, title: string) => {
      const result = await VaultAPI.renameConversation(entryId, title);
      if (!result.ok) {
        notify('error', `Failed to rename entry: ${result.error}`);
        return;
      }
      await reloadEntries();
    },
    [notify, reloadEntries],
  );

  const handleDeleteEntry = useCallback(
    async (entryId: string) => {
      const result = await VaultAPI.deleteConversation(entryId);
      if (!result.ok) {
        notify('error', `Failed to delete entry: ${result.error}`);
        return;
      }
      removeEntry(entryId);
      notify('success', 'Entry deleted.');
    },
    [notify, removeEntry],
  );

  return { handleNewEntry, handleRenameEntry, handleDeleteEntry };
}
