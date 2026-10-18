import { useCallback, useState } from 'react';

import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

/**
 * One inline rename at a time. The Chat sidebar and Explorer's thread list
 * both use it, so a title is checked and saved the same way in each.
 */
export function useConversationRename() {
  const renameConversation = useConversationsStore((state) => state.renameConversation);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [draft, setDraft] = useState('');

  const begin = useCallback((conversationId: string, title: string) => {
    setRenamingId(conversationId);
    setDraft(title);
  }, []);

  const cancel = useCallback(() => {
    setRenamingId(null);
    setDraft('');
  }, []);

  const save = useCallback(async (conversationId: string, currentTitle: string) => {
    const nextTitle = draft.trim();
    if (!nextTitle) {
      toast.warning('Title required', {
        message: 'Conversation title cannot be empty.',
        duration: 2800,
      });
      return;
    }

    if (nextTitle === currentTitle.trim()) {
      cancel();
      return;
    }

    const didRename = await renameConversation(conversationId, nextTitle);
    if (didRename) {
      cancel();
    }
  }, [cancel, draft, renameConversation]);

  return { renamingId, draft, setDraft, begin, cancel, save };
}

export type ConversationRename = ReturnType<typeof useConversationRename>;
