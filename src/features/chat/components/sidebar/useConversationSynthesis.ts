import { useCallback, useMemo, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import {
  Combine,
  MessageSquareShare,
} from 'lucide-react';

import { runSynthesis } from '@/features/journal/synthesis/runSynthesis';
import { selectSynthesisRunning, useSynthesisStore } from '@/features/journal/synthesis/synthesisStore';
import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { PaletteCommand } from '@/stores/paletteCommandsStore';
import { toast } from '@/stores/toastStore';


export function useConversationSynthesis() {
  const queryClient = useQueryClient();
  const { conversations, activeConversationId, continueInNewConversation } = useConversationsStore();
  const [continuingConversationId, setContinuingConversationId] = useState<string | null>(null);
  const isSynthesizing = useSynthesisStore(selectSynthesisRunning);
  const synthesizingConversationId = useSynthesisStore(state => state.job?.status === 'running' && state.job.conversationIds.length === 1 ? state.job.conversationIds[0] : null);
  const synthesizeConversationToJournal = useCallback(async (id: string, title: string) => {
    await runSynthesis({
      title, heading: title,
      request: { conversationIds: [id], scope: 'conversation', maxEntries: 1 },
      destination: { kind: 'capture' },
    }, queryClient);
  }, [queryClient]);
  // A long thread slows every turn and its opening falls out of the model's
  // window. This carries what it established into a fresh chat in the same
  // space; the old one stays as it was.
  const continueConversationInNewChat = useCallback(async (id: string, title: string) => {
    if (continuingConversationId) return;
    setContinuingConversationId(id);
    const working = toast.info(`Summarizing "${title}"…`, {
      message: 'The new chat opens with the summary when it is ready.',
      duration: 0,
    });
    try {
      await continueInNewConversation(id);
      toast.success('Continued in a new chat', { message: `It starts from a summary of "${title}".` });
    } catch (error) {
      toast.error("Couldn't continue in a new chat", { message: error instanceof Error ? error.message : String(error) });
    } finally {
      toast.dismiss(working);
      setContinuingConversationId(null);
    }
  }, [continueInNewConversation, continuingConversationId]);
  const synthesizePaletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'chat.synthesizeToJournal',
        label: 'Synthesize this conversation to Journal',
        group: 'Journal',
        icon: Combine,
        enabled: Boolean(activeConversationId) && !isSynthesizing,
        run: () => {
          if (!activeConversationId) return;
          const conversation = conversations.find((c) => c.id === activeConversationId);
          void synthesizeConversationToJournal(
            activeConversationId,
            conversation?.title ?? 'Conversation',
          );
        },
      },
      {
        id: 'chat.continueInNewChat',
        label: 'Continue this conversation in a new chat',
        group: 'Chat',
        icon: MessageSquareShare,
        enabled: Boolean(activeConversationId) && !continuingConversationId,
        run: () => {
          if (!activeConversationId) return;
          const conversation = conversations.find((c) => c.id === activeConversationId);
          void continueConversationInNewChat(activeConversationId, conversation?.title ?? 'Conversation');
        },
      },
    ],
    [activeConversationId, continueConversationInNewChat, continuingConversationId, conversations, isSynthesizing, synthesizeConversationToJournal],
  );
  useRegisterPaletteCommands(synthesizePaletteCommands);

  return {
    synthesizeConversationToJournal,
    synthesizingConversationId,
    isSynthesizing,
    continueConversationInNewChat,
    continuingConversationId,
  };
}
