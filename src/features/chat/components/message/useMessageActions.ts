import { useCallback, useState } from 'react';

import { useCloudModelCatalogs } from '@/features/model/hooks/useCloudModels';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import type { ChatModelChoice } from '@/lib/bindings';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { GenerationOutcome } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';
import type { DisplayMessage } from '@/types/conversation';
import {
  buildChatModelOptions,
  isUnavailableChatModel,
  resolveEffectiveChatOption,
  sameModelRef,
} from '@/utils/chatModelSelection';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';

/**
 * What to say about a regenerate that failed.
 *
 * A cancelled turn is not a failure and never reaches here — only `'failed'`
 * hands the question back through the composer, so only `'failed'` may say so.
 */
const DEFAULT_CHOICE: ChatModelChoice = { kind: 'default' };

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
  const setConversationModelChoice = useConversationsStore((s) => s.setConversationModelChoice);
  const modelChoice = useConversationsStore(
    (s) => s.conversations.find((conversation) => conversation.id === conversationId)?.modelChoice
  );

  const { downloadedModels } = useDownloadedModels();
  const { catalogs } = useCloudModelCatalogs(false);
  const { data: settings } = useSettingsQuery();

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

  /**
   * Answer this question again with another model, once. Nothing is switched:
   * the chat keeps its own model and Settings keeps the default, so there is
   * nothing to switch back. A good answer can become the chat's model in one
   * click.
   */
  const handleTryWithModel = useCallback(
    async (modelId: string, modelLabel: string) => {
      if (!conversationId) return;
      const option = buildChatModelOptions(settings?.llm, downloadedModels, catalogs).find((o) => o.id === modelId);
      if (!option) {
        toast.error("Couldn't regenerate", {
          message: `${modelLabel} is no longer available. Pick another model.`,
        });
        return;
      }

      const outcome = await regenerateResponse(conversationId, { model: option.ref });
      if (outcome === 'cancelled') {
        // You stopped it. That is not an error, and it is not an answer.
        toast.info(`Stopped ${modelLabel}`, {
          message: "You stopped the answer. This chat's model is unchanged.",
        });
        return;
      }
      if (outcome !== 'answered') {
        toast.error("Couldn't regenerate", { message: regenerateFailureMessage(outcome) });
        return;
      }

      // Offer to keep it only when it is not what this chat already uses.
      const current = resolveEffectiveChatOption(modelChoice ?? DEFAULT_CHOICE, settings?.llm, downloadedModels, catalogs);
      const isChatsModel = current !== null && !isUnavailableChatModel(current) && sameModelRef(current.ref, option.ref);
      toast.success(
        `Answered with ${modelLabel}`,
        isChatsModel
          ? undefined
          : {
              action: {
                label: `Use ${modelLabel} for this chat`,
                onClick: () => {
                  void setConversationModelChoice(conversationId, { kind: 'explicit', model: option.ref })
                    .then(() => toast.success(`This chat now uses ${modelLabel}`))
                    .catch((error: unknown) =>
                      toast.error("Couldn't change this chat's model", {
                        message: error instanceof Error ? error.message : String(error),
                      })
                    );
                },
              },
            }
      );
    },
    [conversationId, downloadedModels, catalogs, modelChoice, regenerateResponse, setConversationModelChoice, settings?.llm]
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
    modelChoice: modelChoice ?? DEFAULT_CHOICE,
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
