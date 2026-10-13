import type { RefObject } from 'react';

import { Cpu, GitBranch, Library, Paperclip, RefreshCw, Scissors, ScrollText } from 'lucide-react';

import type { useComposer } from '@/features/chat/hooks/useComposer';
import type { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';

/** Everything the chat's palette commands read or call. */
export interface ChatPaletteCommandInputs {
  activeConversationId: string | null;
  activeModelLabel: string | null;
  canRegenerate: boolean;
  downloadedModels: ReturnType<typeof useDownloadedModels>['downloadedModels'];
  forkConversation: ConversationsState['forkConversation'];
  chooseFiles: () => Promise<unknown>;
  handleCompact: (conversationId: string) => Promise<void>;
  handleSearchGeneral: () => Promise<void>;
  conversationSpaceName: string | null;
  isCompacting: boolean;
  isScopedToLinkedFiles: boolean;
  messageCount: number;
  regenerateResponse: ConversationsState['regenerateResponse'];
  resolveEffectiveToolPreferences: ReturnType<typeof useComposer>['resolveEffectiveToolPreferences'];
  setIsMemoryOpen: (isOpen: boolean) => void;
  modelLabelRef: RefObject<HTMLButtonElement | null>;
}

/** The chat's commands in the palette: regenerate, branch, compact, memory, model, files, space. */
export function chatPaletteCommands({
  activeConversationId,
  activeModelLabel,
  canRegenerate,
  downloadedModels,
  forkConversation,
  chooseFiles,
  handleCompact,
  handleSearchGeneral,
  conversationSpaceName,
  isCompacting,
  isScopedToLinkedFiles,
  messageCount,
  regenerateResponse,
  resolveEffectiveToolPreferences,
  setIsMemoryOpen,
  modelLabelRef,
}: ChatPaletteCommandInputs): PaletteCommand[] {
  return [
    {
      id: 'chat.regenerate',
      label: 'Regenerate answer',
      group: 'Chat',
      icon: RefreshCw,
      enabled: canRegenerate,
      run: () => {
        if (!activeConversationId) return;
        // Re-run the turn the composer is describing, not a default one.
        void regenerateResponse(
          activeConversationId,
          resolveEffectiveToolPreferences()
        ).then((outcome) => {
          if (outcome === 'answered' || outcome === 'cancelled') return;
          toast.error("Couldn't regenerate", {
            message:
              outcome === 'busy'
                ? 'This conversation is still answering.'
                : 'Your question is back in the composer.',
          });
        });
      },
    },
    {
      id: 'chat.branch',
      label: 'Branch this conversation',
      group: 'Chat',
      icon: GitBranch,
      enabled: Boolean(activeConversationId) && messageCount > 0,
      run: () => {
        if (activeConversationId) {
          void forkConversation(activeConversationId).then((newId) => {
            if (newId) toast.success('Branched', { message: "You're in the new conversation." });
          });
        }
      },
    },
    {
      id: 'chat.compact',
      label: 'Compact context',
      group: 'Chat',
      icon: Scissors,
      enabled: Boolean(activeConversationId) && !isCompacting && messageCount > 0,
      description: 'Fold older messages into a summary (/compact).',
      run: () => {
        if (activeConversationId) void handleCompact(activeConversationId);
      },
    },
    {
      id: 'chat.memory',
      label: 'Show conversation memory',
      group: 'Chat',
      icon: ScrollText,
      enabled: Boolean(activeConversationId),
      description: 'What was recorded, and the quotation behind each item.',
      run: () => {
        setIsMemoryOpen(true);
      },
    },
    {
      id: 'chat.switch-model',
      label: 'Switch chat model',
      group: 'Chat',
      icon: Cpu,
      // The picker hangs off the active-model label, so the verb only works
      // when that label is on screen. No verb that does nothing.
      enabled:
        Boolean(activeModelLabel) &&
        downloadedModels.some((model) => model.model_type === 'language_model'),
      run: () => {
        modelLabelRef.current?.click();
      },
    },
    {
      id: 'chat.add-files',
      label: 'Add files to this conversation',
      group: 'Chat',
      icon: Paperclip,
      enabled: Boolean(activeConversationId),
      description: 'Or drop them onto the conversation.',
      run: () => {
        void chooseFiles();
      },
    },
    {
      id: 'chat.search-general',
      label: 'Move this chat to General',
      group: 'Chat',
      icon: Library,
      description: conversationSpaceName
        ? `It searches ${conversationSpaceName} now.`
        : undefined,
      enabled: isScopedToLinkedFiles,
      run: () => {
        void handleSearchGeneral();
      },
    },
  ];
}
