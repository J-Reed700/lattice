import { useCallback } from 'react';

import type { useComposer } from '@/features/chat/hooks/useComposer';
import type { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

type DownloadedModels = ReturnType<typeof useDownloadedModels>;

/** Moving the open chat to another space, and switching the app-wide chat model. */
export function useChatSwitchActions({
  activeConversationId,
  applyFocusDocuments,
  activeModel,
  setActiveChatModel,
}: {
  activeConversationId: string | null;
  applyFocusDocuments: ReturnType<typeof useComposer>['applyFocusDocuments'];
  activeModel: DownloadedModels['activeModel'];
  setActiveChatModel: DownloadedModels['setActiveChatModel'];
}) {
  const moveConversationToSpace = useConversationsStore((state) => state.moveConversationToSpace);

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

  const handleSwitchActiveModel = useCallback(
    async (modelId: string, modelLabel: string) => {
      const previousModelId = activeModel?.model_id ?? null;
      const previousLabel = activeModel?.model_name ?? previousModelId;
      try {
        await setActiveChatModel(modelId);
      } catch (error) {
        toast.error("Couldn't switch model", {
          message: error instanceof Error ? error.message : String(error),
        });
        return;
      }
      // The switch is app-wide, so always offer the way back — same as the
      // "Try with another model" toast on a message.
      toast.success(`Switched to ${modelLabel}`, {
        ...(previousModelId && previousModelId !== modelId
          ? {
              action: {
                label: 'Switch back',
                onClick: () => {
                  void setActiveChatModel(previousModelId).then(() => {
                    toast.success(`Back to ${previousLabel}`);
                  });
                },
              },
            }
          : {}),
      });
    },
    [activeModel, setActiveChatModel]
  );

  return { handleChangeSpace, handleSearchGeneral, handleSwitchActiveModel };
}
