import { useCallback, useState } from 'react';

import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { GenerationOutcome } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';
import type { LLMSettings } from '@/types/api/settings';
import type { DisplayMessage } from '@/types/conversation';
import {
  buildChatModelOptions,
  LLAMACPP_SERVER_MODEL_ID,
  OLLAMA_SERVER_MODEL_ID,
  resolveActiveChatOption,
} from '@/utils/chatModelSelection';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';

/**
 * What to say about a regenerate that failed.
 *
 * A cancelled turn is not a failure and never reaches here — only `'failed'`
 * hands the question back through the composer, so only `'failed'` may say so.
 */
function regenerateFailureMessage(
  outcome: Exclude<GenerationOutcome, 'answered' | 'cancelled'>
): string {
  if (outcome === 'busy') return 'This conversation is still answering.';
  return 'Your question is back in the composer.';
}

/** What a message's actions do, and whether it is being edited: copy, reference, delete, regenerate, branch and edit. */
export function useMessageActions({
  message,
  messageId,
  conversationId,
  conversationSpaceId,
  previousMessageId,
  activeConversationId,
  isMessageBookmarked,
  normalizedMarkdownContent,
}: {
  message: DisplayMessage;
  messageId: string | undefined;
  conversationId: string | undefined;
  conversationSpaceId: string | null;
  previousMessageId: string | undefined;
  activeConversationId: string | null;
  isMessageBookmarked: boolean;
  normalizedMarkdownContent: string;
}) {
  const [isEditing, setIsEditing] = useState(false);
  const bookmarkMessage = useConversationsStore((s) => s.bookmarkMessage);
  const unbookmarkMessage = useConversationsStore((s) => s.unbookmarkMessage);
  const deleteMessage = useConversationsStore((s) => s.deleteMessage);
  const regenerateResponse = useConversationsStore((s) => s.regenerateResponse);
  const truncateAfter = useConversationsStore((s) => s.truncateAfter);
  const forkConversation = useConversationsStore((s) => s.forkConversation);
  const sendMessage = useConversationsStore((s) => s.sendMessage);
  const createConversation = useConversationsStore((s) => s.createConversation);

  const { activeModel, setActiveChatModel, downloadedModels = [] } = useDownloadedModels();
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  const updateProvider = useCallback(
    async (provider: LLMSettings['provider']) => {
      await updateSettings({ category: 'llm', updates: { provider } });
    },
    [updateSettings]
  );

  const handleCopy = async () => {
    await navigator.clipboard.writeText(normalizedMarkdownContent);
  };

  const handleBookmarkToggle = async () => {
    if (!activeConversationId || !messageId) return;

    if (isMessageBookmarked) {
      await unbookmarkMessage(activeConversationId, messageId);
      return;
    }

    const compact = message.content.replace(/\s+/g, ' ').trim();
    const title = compact.length > 80 ? `${compact.slice(0, 80)}...` : compact;
    if (!(await bookmarkMessage(activeConversationId, messageId, title || null, null))) return;
    // Say where it went and what it is for; a silent save teaches nothing.
    toast.success('Saved', { message: 'Lattice will use this in future answers.' });
  };

  const handleRegenerate = useCallback(async () => {
    if (!conversationId) return;
    const outcome = await regenerateResponse(conversationId);
    if (outcome === 'answered' || outcome === 'cancelled') return;
    // Only a real failure puts the question back in the composer; saying so
    // when it did not happen would send the reader looking for it.
    toast.error("Couldn't regenerate", { message: regenerateFailureMessage(outcome) });
  }, [conversationId, regenerateResponse]);

  const handleTryWithModel = useCallback(
    async (modelId: string, modelLabel: string) => {
      if (!conversationId) return;
      const previousOption = resolveActiveChatOption(
        settings?.llm,
        downloadedModels,
        activeModel?.model_id ?? null
      );
      const previousProvider = settings?.llm.provider;
      const previousLocalModelId = activeModel?.model_id ?? null;
      const previousLabel = previousOption?.triggerLabel ?? activeModel?.model_name ?? previousLocalModelId;

      try {
        const targetOption = buildChatModelOptions(settings?.llm, downloadedModels).find(
          (o) => o.id === modelId
        );

        if (modelId === LLAMACPP_SERVER_MODEL_ID || targetOption?.backend === 'llamacpp') {
          await updateProvider('llamacpp');
        } else if (modelId === OLLAMA_SERVER_MODEL_ID || targetOption?.backend === 'ollama') {
          if (targetOption?.downloadedModelId || downloadedModels.some((m) => m.model_id === modelId)) {
            await setActiveChatModel(modelId);
          }
          if (settings?.llm && settings.llm.provider !== 'ollama') {
            await updateProvider('ollama');
          }
        } else {
          await setActiveChatModel(modelId);
          if (settings?.llm && settings.llm.provider !== 'local') {
            await updateProvider('local');
          }
        }
      } catch (error) {
        toast.error("Couldn't switch model", {
          message: error instanceof Error ? error.message : String(error),
        });
        return;
      }

      const canSwitchBack = Boolean(
        (previousLocalModelId && previousLocalModelId !== modelId) ||
        (previousOption && previousOption.id !== modelId) ||
        (previousProvider && settings?.llm)
      );

      // The switch is app-wide, so every outcome offers the way back.
      const switchBack = canSwitchBack
        ? {
            action: {
              label: 'Switch back',
              onClick: () => {
                void (async () => {
                  if (
                    previousLocalModelId &&
                    previousLocalModelId !== modelId &&
                    (previousOption?.downloadedModelId || !settings?.llm)
                  ) {
                    await setActiveChatModel(previousLocalModelId);
                  }
                  if (previousProvider && settings?.llm) {
                    await updateProvider(previousProvider);
                  }
                  toast.success(`Back to ${previousLabel}`);
                })();
              },
            },
          }
        : {};

      const outcome = await regenerateResponse(conversationId);
      if (outcome === 'cancelled') {
        // You stopped it. That is not an error, and it is not an answer.
        toast.info(`Switched to ${modelLabel}`, {
          message: 'You stopped the answer.',
          ...switchBack,
        });
        return;
      }
      if (outcome !== 'answered') {
        // The model did switch, but no answer came back. Saying "Answered
        // with X" here would be a claim about output that does not exist.
        toast.error("Couldn't regenerate", {
          message: regenerateFailureMessage(outcome),
          ...switchBack,
        });
        return;
      }
      toast.success(`Answered with ${modelLabel}`, switchBack);
    },
    [conversationId, activeModel, downloadedModels, setActiveChatModel, settings, updateProvider, regenerateResponse]
  );

  const handleBranch = useCallback(async () => {
    if (!conversationId || !messageId) return;
    const newId = await forkConversation(conversationId, messageId);
    if (newId) {
      toast.success('Branched', { message: "You're in the new conversation." });
    }
  }, [conversationId, messageId, forkConversation]);

  const handleEditSend = useCallback(
    async (value: string) => {
      if (!conversationId || !messageId) return;
      setIsEditing(false);
      const truncated = await truncateAfter(conversationId, messageId, true);
      if (!truncated) return;
      await sendMessage(value, conversationId);
    },
    [conversationId, messageId, truncateAfter, sendMessage]
  );

  const handleEditSendAsBranch = useCallback(
    async (value: string) => {
      if (!conversationId) return;
      setIsEditing(false);
      // No previous message means this is the first turn: a fork of nothing is
      // a new conversation, so make one rather than copying the whole thread.
      // Either way the branch stays in this conversation's space. A fork copies
      // it; a new conversation would otherwise take whatever the sidebar has
      // selected, and the branch would then search a different library.
      const branchId = previousMessageId
        ? await forkConversation(conversationId, previousMessageId)
        : await createConversation(createDefaultConversationTitle(), conversationSpaceId);
      if (!branchId) return;
      await sendMessage(value, branchId);
      toast.success('Branched', { message: "You're in the new conversation." });
    },
    [conversationId, conversationSpaceId, previousMessageId, forkConversation, createConversation, sendMessage]
  );

  const handleDeleteMessage = async () => {
    if (!messageId || !('conversationId' in message)) return;

    const confirmed = window.confirm(
      "Delete this message? This can't be undone."
    );
    if (!confirmed) return;

    await deleteMessage(message.conversationId, messageId);
  };

  return {
    isEditing,
    setIsEditing,
    activeModel,
    handleCopy,
    handleBookmarkToggle,
    handleRegenerate,
    handleTryWithModel,
    handleBranch,
    handleEditSend,
    handleEditSendAsBranch,
    handleDeleteMessage,
  };
}
