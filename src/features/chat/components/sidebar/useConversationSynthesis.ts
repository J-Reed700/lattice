import { useCallback, useMemo, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import {
  Combine,
  MessageSquareShare,
} from 'lucide-react';

import { runSynthesis } from '@/features/journal/synthesis/runSynthesis';
import { useActiveSynthesis } from '@/features/journal/synthesis/synthesisPanel';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';


export function useConversationSynthesis() {
  const queryClient = useQueryClient();
  const { conversations, activeConversationId, continueInNewConversation } = useConversationsStore();
  const [continuingConversationId, setContinuingConversationId] = useState<string | null>(null);
  const activeSynthesis = useActiveSynthesis();
  const isSynthesizing = activeSynthesis !== null;
  const synthesizingConversationId = activeSynthesis?.conversationIds.length === 1 ? activeSynthesis.conversationIds[0] : null;
  const synthesizeConversationToJournal = useCallback(async (id: string, title: string) => {
    try {
      await runSynthesis({
        title, heading: title,
        request: { conversationIds: [id], scope: 'conversation', maxEntries: 1 },
        destination: { kind: 'capture' },
      }, queryClient);
    } catch (error) {
      toast.error("Couldn't start the synthesis", { message: error instanceof Error ? error.message : String(error) });
    }
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
