import { useEffect, useMemo, useState } from 'react';

import { useNavigate } from 'react-router';

import { useConversationsStore } from '@/stores/conversationsStore';
import { toast } from '@/stores/toastStore';

import { JOURNAL_SPACE_DEFAULT_ACCENT, JOURNAL_SPACE_DEFAULT_ICON } from './sidebarUtils';
import { useAddConversationsToJournalMutation, useCreateJournalMutation, useJournalsQuery } from './workspaceQueries';

export function useJournalSelection() {
  const navigate = useNavigate();
  const { conversations, spaces, filterMode, selectedSpaceId, activeConversationId } = useConversationsStore();
  const { journals } = useJournalsQuery();
  const journalSpaces = useMemo(() => journals.filter(journal => !journal.isArchived), [journals]);
  const journalNameById = useMemo(() => new Map(journals.map(journal => [journal.id, journal.name])), [journals]);
  const spaceNameById = useMemo(() => new Map(spaces.map(space => [space.id, space.name])), [spaces]);
  const createJournal = useCreateJournalMutation();
  const addToJournal = useAddConversationsToJournalMutation();
  const [isSelectionMode, setIsSelectionMode] = useState(false);
  const [selectedConversationIds, setSelectedConversationIds] = useState<Set<string>>(new Set());
  const [bulkSpaceIdDraft, setBulkSpaceIdDraft] = useState('');
  const [isBulkMoving, setIsBulkMoving] = useState(false);
  const [isCreatingQuickJournal, setIsCreatingQuickJournal] = useState(false);
  const selectedConversationCount = selectedConversationIds.size;
  const hasConversations = conversations.length > 0;
  const areAllVisibleConversationsSelected = hasConversations
    && conversations.every((conversation) => selectedConversationIds.has(conversation.id));

  useEffect(() => {
    if (!isSelectionMode) {
      setSelectedConversationIds(new Set());
      setBulkSpaceIdDraft('');
      return;
    }

  }, [isSelectionMode]);

  useEffect(() => {
    if (!isSelectionMode) return;
    const visibleConversationIds = new Set(conversations.map((conversation) => conversation.id));
    setSelectedConversationIds((prev) => {
      const next = new Set<string>();
      prev.forEach((conversationId) => {
        if (visibleConversationIds.has(conversationId)) {
          next.add(conversationId);
        }
      });
      return next;
    });
  }, [conversations, isSelectionMode]);

  useEffect(() => {
    if (filterMode !== 'snippets') return;
    setIsSelectionMode(false);
  }, [filterMode]);

  useEffect(() => {
    if (!isSelectionMode) return;
    if (bulkSpaceIdDraft) {
      const existing = journalSpaces.some((space) => space.id === bulkSpaceIdDraft);
      if (existing) return;
    }

    const selectedIsJournal = selectedSpaceId
      ? journalSpaces.some((space) => space.id === selectedSpaceId)
      : false;
    const fallbackSpaceId = selectedIsJournal
      ? selectedSpaceId ?? ''
      : (journalSpaces[0]?.id ?? '');
    setBulkSpaceIdDraft(fallbackSpaceId);
  }, [
    bulkSpaceIdDraft,
    isSelectionMode,
    journalSpaces,
    selectedSpaceId,
  ]);

  const toggleConversationSelection = (conversationId: string) => {
    setSelectedConversationIds((prev) => {
      const next = new Set(prev);
      if (next.has(conversationId)) {
        next.delete(conversationId);
      } else {
        next.add(conversationId);
      }
      return next;
    });
  };

  const toggleSelectAllVisibleConversations = () => {
    if (!hasConversations) return;

    if (areAllVisibleConversationsSelected) {
      setSelectedConversationIds(new Set());
      return;
    }

    setSelectedConversationIds(new Set(conversations.map((conversation) => conversation.id)));
  };

  const addSelectedConversationsToJournal = async () => {
    const targetSpaceId = bulkSpaceIdDraft.trim();
    const selectedIds = Array.from(selectedConversationIds);

    if (selectedIds.length === 0) {
      toast.warning('No conversations selected');
      return;
    }
    if (!targetSpaceId) {
      toast.warning('Choose a destination journal');
      return;
    }
    if (!journalSpaces.some((journal) => journal.id === targetSpaceId)) {
      toast.warning('Choose a destination journal');
      return;
    }

    const destinationName =
      journalNameById.get(targetSpaceId) ?? spaceNameById.get(targetSpaceId) ?? 'selected journal';
    setIsBulkMoving(true);
    try {
      const moveResults = await addToJournal.mutateAsync({ ids: selectedIds, journalId: targetSpaceId });

      const failedMoves = moveResults.filter(({ result }) => !result.ok);
      const addedCount = selectedIds.length - failedMoves.length;

      if (failedMoves.length === 0) {
        toast.success('Added to journal', {
          message: `${addedCount} added to ${destinationName}. Conversations stay in their current space.`,
          duration: 3200,
        });
        setSelectedConversationIds(new Set());
        setIsSelectionMode(false);
        return;
      }

      setSelectedConversationIds(
        new Set(failedMoves.map(({ conversationId }) => conversationId))
      );
      toast.warning('Some conversations were not added', {
        message: `${addedCount}/${selectedIds.length} added to ${destinationName}.`,
        duration: 4500,
      });
    } catch (error) {
      toast.error('Could not update journal', { message: error instanceof Error ? error.message : String(error) });
    } finally {
      setIsBulkMoving(false);
    }
  };

  const createQuickJournal = async () => {
    if (isCreatingQuickJournal) return;

    setIsCreatingQuickJournal(true);
    try {
      const existingNames = new Set(journals.map((journal) => journal.name.toLowerCase()));
      let idx = journals.length + 1;
      let name = `Journal ${idx}`;
      while (existingNames.has(name.toLowerCase())) {
        idx += 1;
        name = `Journal ${idx}`;
      }

      const result = await createJournal.mutateAsync({
        name,
        description: null,
        icon: JOURNAL_SPACE_DEFAULT_ICON,
        accentColor: JOURNAL_SPACE_DEFAULT_ACCENT,
        spacePrompt: null,
        defaultModelName: null,
        toolPreferencesJson: null,
      });
      setBulkSpaceIdDraft(result.id);
      toast.success('Journal created', {
        message: `${result.name} is ready.`,
        duration: 2600,
      });
    } catch (error) {
      toast.error('Could not update journal', { message: error instanceof Error ? error.message : String(error) });
    } finally {
      setIsCreatingQuickJournal(false);
    }
  };

  const openSelectedJournalNotebook = () => {
    const targetJournalId = (
      bulkSpaceIdDraft.trim()
      && journalSpaces.some((journal) => journal.id === bulkSpaceIdDraft.trim())
    )
      ? bulkSpaceIdDraft.trim()
      : (journalSpaces[0]?.id ?? null);
    if (!targetJournalId) {
      return;
    }

    const params = new URLSearchParams({
      journalSpaceId: targetJournalId,
      panel: 'entries',
    });
    if (activeConversationId) {
      params.set('entryId', activeConversationId);
    }
    navigate(`/journals?${params.toString()}`);
  };

  return {
    isSelectionMode,
    setIsSelectionMode,
    selectedConversationIds,
    selectedConversationCount,
    bulkSpaceIdDraft,
    setBulkSpaceIdDraft,
    isBulkMoving,
    isCreatingQuickJournal,
    hasConversations,
    areAllVisibleConversationsSelected,
    toggleConversationSelection,
    toggleSelectAllVisibleConversations,
    addSelectedConversationsToJournal,
    createQuickJournal,
    openSelectedJournalNotebook,
  };
}
