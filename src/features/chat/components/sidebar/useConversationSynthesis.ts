import { useCallback, useMemo, useState } from 'react';

import {
  Combine,
  MessageSquareShare,
} from 'lucide-react';
import { useNavigate } from 'react-router';

import { useSynthesizeConversationMutation } from '@/features/chat/components/sidebar/workspaceQueries';
import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { PaletteCommand } from '@/stores/paletteCommandsStore';
import { toast } from '@/stores/toastStore';


export function useConversationSynthesis() {
  const navigate = useNavigate();
  const { conversations, activeConversationId, continueInNewConversation } = useConversationsStore();
  const [continuingConversationId, setContinuingConversationId] = useState<string | null>(null);
  const mutation = useSynthesizeConversationMutation();
  const { mutateAsync } = mutation;
  const synthesizeConversationToJournal = useCallback(async (id: string, title: string) => {
    // A synthesis is two full generations — minutes on a local model. Started
    // from the palette there is no row spinner to look at, so without this the
    // command appears to do nothing until the "saved" toast arrives.
    const working = toast.info(`Synthesizing "${title}"…`, {
      message: 'This takes a few minutes. The page opens when it is ready.',
      duration: 0,
    });
    try {
      const capture = await mutateAsync({ id, title });
      toast.success(`Synthesis saved to "${capture.noteTitle}"`);
      navigate(`/journals?${new URLSearchParams({ noteId: capture.noteId }).toString()}`);
    } catch (error) {
      toast.error('Synthesis failed', { message: error instanceof Error ? error.message : String(error) });
    } finally {
      toast.dismiss(working);
    }
  }, [mutateAsync, navigate]);
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
    [activeConversationId, continueConversationInNewChat, continuingConversationId, conversations, synthesizeConversationToJournal],
  );
  useRegisterPaletteCommands(synthesizePaletteCommands);

  return {
    synthesizeConversationToJournal,
    synthesizingConversationId: mutation.isPending ? mutation.variables.id : null,
    continueConversationInNewChat,
    continuingConversationId,
  };
}
