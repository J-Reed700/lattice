import { useState } from 'react';

import { useNavigate } from 'react-router';

import {
  JOURNAL_SPACE_DEFAULT_ACCENT,
  JOURNAL_SPACE_DEFAULT_ICON,
  type SpaceKind,
} from '@/features/chat/components/sidebar/sidebarUtils';
import { useCreateJournalMutation, useJournalsQuery } from '@/features/chat/components/sidebar/workspaceQueries';
import type { useSpaceEditor } from '@/features/spaces/hooks/useSpaceEditor';
import { uniqueName } from '@/features/spaces/model/spaces';
import { useConversationsStore } from '@/stores/conversationsStore';
import { toast } from '@/stores/toastStore';

/**
 * The Spaces panel's "New space" form, which makes a space or a journal.
 * A space goes through the space editor; a journal opens in the Journal.
 */
export function useNewSpaceForm(editor: ReturnType<typeof useSpaceEditor>) {
  const navigate = useNavigate();
  const { setSelectedSpace, loadConversations } = useConversationsStore();
  const { journals } = useJournalsQuery();
  const createJournal = useCreateJournalMutation();
  const [isCreateSpaceOpen, setIsCreateSpaceOpen] = useState(false);
  const [newSpaceKindDraft, setNewSpaceKindDraft] = useState<SpaceKind>('standard');
  const [newSpaceNameDraft, setNewSpaceNameDraft] = useState('');

  const reset = () => {
    setIsCreateSpaceOpen(false);
    setNewSpaceNameDraft('');
    setNewSpaceKindDraft('standard');
  };

  const createSpace = async () => {
    if (newSpaceKindDraft === 'standard') {
      if (await editor.createSpace(newSpaceNameDraft)) reset();
      return;
    }
    const name = uniqueName(newSpaceNameDraft, journals.map((journal) => journal.name), 'Journal');
    try {
      const result = await createJournal.mutateAsync({
        name,
        description: null,
        icon: JOURNAL_SPACE_DEFAULT_ICON,
        accentColor: JOURNAL_SPACE_DEFAULT_ACCENT,
        spacePrompt: null,
        defaultModelName: null,
        toolPreferencesJson: null,
      });
      setSelectedSpace(null);
      await loadConversations({ spaceId: null });
      navigate(`/journals?journalSpaceId=${encodeURIComponent(result.id)}&panel=entries`);
      reset();
      toast.success('Journal created', { message: name });
    } catch (error) {
      toast.error('Could not update space', { message: error instanceof Error ? error.message : String(error) });
    }
  };

  return {
    isCreateSpaceOpen,
    setIsCreateSpaceOpen,
    newSpaceKindDraft,
    setNewSpaceKindDraft,
    newSpaceNameDraft,
    setNewSpaceNameDraft,
    isCreatingSpace: editor.isCreatingSpace || createJournal.isPending,
    createSpace,
  };
}
