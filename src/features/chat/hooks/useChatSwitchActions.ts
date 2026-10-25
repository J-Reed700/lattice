import { useCallback } from 'react';

import type { useComposer } from '@/features/chat/hooks/useComposer';
import { useCloudModelCatalogs } from '@/features/model/hooks/useCloudModels';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import type { ChatModelChoice } from '@/lib/bindings';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import { buildChatModelOptions, resolveDefaultChatOption, sameModelRef } from '@/utils/chatModelSelection';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';

const DEFAULT_CHOICE: ChatModelChoice = { kind: 'default' };

/**
 * Moving the open chat to another space, and choosing the model this chat
 * answers with. A choice here changes this chat and nothing else: the
 * app-wide default is set in Settings, never from a chat.
 */
export function useChatSwitchActions({
  activeConversationId,
  applyFocusDocuments,
}: {
  activeConversationId: string | null;
  applyFocusDocuments: ReturnType<typeof useComposer>['applyFocusDocuments'];
}) {
  const moveConversationToSpace = useConversationsStore((state) => state.moveConversationToSpace);
  const setConversationModelChoice = useConversationsStore((state) => state.setConversationModelChoice);
  const createConversation = useConversationsStore((state) => state.createConversation);
  const conversations = useConversationsStore((state) => state.conversations);
  const { data: settings } = useSettingsQuery();
  const { downloadedModels } = useDownloadedModels();
  const { catalogs } = useCloudModelCatalogs(false);

  /**
   * Move this chat to another space, which changes the documents every later
   * answer can draw on. The toast names the space: General is a space like any
   * other, and calling a move there "searching your whole vault" promised
   * documents filed elsewhere that it cannot reach.
   */
  const handleChangeSpace = useCallback(
    async (spaceId: string, spaceName: string) => {
      if (!activeConversationId) return;
      // A document pinned in the old space is not in the new one, and focus
      // fails closed: keeping it would leave the chat asking nothing at all.
      applyFocusDocuments([]);
      await moveConversationToSpace(activeConversationId, spaceId);
      toast.success(`Now searching ${spaceName}`, {
        message: 'Answers from here on use the documents filed there.',
      });
    },
    [activeConversationId, applyFocusDocuments, moveConversationToSpace]
  );

  const handleSearchGeneral = useCallback(
    () => handleChangeSpace(GENERAL_SPACE_ID, 'General'),
    [handleChangeSpace]
  );

  /**
   * Save a choice on this chat, making the chat first when none is open: a
   * pick in the empty state belongs to the chat the first message will land
   * in. Resolves false when nothing was saved; the toast says why.
   */
  const saveChoice = useCallback(
    async (choice: ChatModelChoice): Promise<boolean> => {
      try {
        const conversationId =
          activeConversationId ?? (await createConversation(createDefaultConversationTitle()));
        await setConversationModelChoice(conversationId, choice);
        return true;
      } catch (error) {
        toast.error("Couldn't change this chat's model", {
          message: error instanceof Error ? error.message : String(error),
        });
        return false;
      }
    },
    [activeConversationId, createConversation, setConversationModelChoice]
  );

  const currentChoice =
    conversations.find((conversation) => conversation.id === activeConversationId)?.modelChoice ??
    DEFAULT_CHOICE;

  /** Answer this chat with the model of the picked row, from its next turn on. */
  const handleChooseChatModel = useCallback(
    async (modelId: string, modelLabel: string) => {
      const option = buildChatModelOptions(settings?.llm, downloadedModels, catalogs).find((o) => o.id === modelId);
      if (!option) {
        toast.error("Couldn't change this chat's model", {
          message: `${modelLabel} is no longer available. Pick another model.`,
        });
        return;
      }
      if (currentChoice.kind === 'explicit' && sameModelRef(currentChoice.model, option.ref)) return;
      if (await saveChoice({ kind: 'explicit', model: option.ref })) {
        toast.success(`This chat now uses ${modelLabel}`);
      }
    },
    [currentChoice, downloadedModels, catalogs, saveChoice, settings?.llm]
  );

  /** Put this chat back on the Settings default, which it then follows. */
  const handleUseDefault = useCallback(async () => {
    // A chat starts on the default, so with none open there is nothing to change.
    if (!activeConversationId || currentChoice.kind === 'default') return;
    if (!(await saveChoice(DEFAULT_CHOICE))) return;
    const defaultOption = resolveDefaultChatOption(settings?.llm, downloadedModels, catalogs);
    toast.success(
      defaultOption
        ? `This chat now uses the app default (${defaultOption.triggerLabel})`
        : 'This chat now uses the app default',
      defaultOption ? undefined : { message: 'No chat model is selected in Settings → Models.' }
    );
  }, [activeConversationId, currentChoice.kind, downloadedModels, catalogs, saveChoice, settings?.llm]);

  return { handleChangeSpace, handleSearchGeneral, handleChooseChatModel, handleUseDefault };
}
