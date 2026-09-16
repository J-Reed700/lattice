import { useCallback, useMemo } from 'react';

import {
  Combine
} from 'lucide-react';
import { useNavigate } from 'react-router';

import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { PaletteCommand } from '@/stores/paletteCommandsStore';
import { toast } from '@/stores/toastStore';

import { useSynthesizeConversationMutation } from './workspaceQueries';

export function useConversationSynthesis() {
  const navigate = useNavigate();
  const { conversations, activeConversationId } = useConversationsStore();
  const mutation = useSynthesizeConversationMutation();
  const { mutateAsync } = mutation;
  const synthesizeConversationToJournal = useCallback(async (id: string, title: string) => {
    try {
      const capture = await mutateAsync({ id, title });
      toast.success(`Synthesis saved to "${capture.noteTitle}"`);
      navigate(`/journals?${new URLSearchParams({ noteId: capture.noteId }).toString()}`);
    } catch (error) {
      toast.error('Synthesis failed', { message: error instanceof Error ? error.message : String(error) });
    }
  }, [mutateAsync, navigate]);
  const synthesizePaletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'chat.synthesizeToJournal',
        label: 'Synthesize this conversation to Journal',
        group: 'Journal',
        icon: Combine,
        enabled: Boolean(activeConversationId),
        run: () => {
          if (!activeConversationId) return;
          const conversation = conversations.find((c) => c.id === activeConversationId);
          void synthesizeConversationToJournal(
            activeConversationId,
            conversation?.title ?? 'Conversation',
          );
        },
      },
    ],
    [activeConversationId, conversations, synthesizeConversationToJournal],
  );
  useRegisterPaletteCommands(synthesizePaletteCommands);

  return {
    synthesizeConversationToJournal,
    synthesizingConversationId: mutation.isPending ? mutation.variables.id : null,
  };
}
