import { useCallback, useContext } from 'react';

import { QueryClientContext } from '@tanstack/react-query';

import type { useComposer } from '@/features/chat/hooks/useComposer';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';
import {
  buildChatModelOptions,
  LLAMACPP_SERVER_MODEL_ID,
  OLLAMA_SERVER_MODEL_ID,
  resolveActiveChatOption,
} from '@/utils/chatModelSelection';

type DownloadedModels = ReturnType<typeof useDownloadedModels>;

/** Moving the open chat to another space, and switching the app-wide chat model. */
export function useChatSwitchActions({
  activeConversationId,
  applyFocusDocuments,
  activeModel,
  setActiveChatModel,
  downloadedModels: propDownloadedModels,
}: {
  activeConversationId: string | null;
  applyFocusDocuments: ReturnType<typeof useComposer>['applyFocusDocuments'];
  activeModel: DownloadedModels['activeModel'];
  setActiveChatModel: DownloadedModels['setActiveChatModel'];
  downloadedModels?: DownloadedModel[];
}) {
  const moveConversationToSpace = useConversationsStore((state) => state.moveConversationToSpace);
  const { data: settings } = useSettingsQuery();
  const queryClient = useContext(QueryClientContext);
  const updateProvider = useCallback(
    async (provider: LLMSettings['provider']) => {
      const result = await VaultAPI.updateSettings({ category: 'llm', updates: { provider } });
      if (!result.ok) throw new Error(result.error);
      if (queryClient) {
        queryClient.setQueryData(['settings'], result.data);
        void queryClient.invalidateQueries({ queryKey: ['settings'] });
      }
    },
    [queryClient]
  );
  const { downloadedModels: hookDownloadedModels = [] } = useDownloadedModels();
  const downloadedModels = propDownloadedModels ?? hookDownloadedModels;

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
        if (targetOption?.isActive && previousOption?.id === modelId) {
          return;
        }

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

      // The switch is app-wide, so always offer the way back — same as the
      // "Try with another model" toast on a message.
      toast.success(`Switched to ${modelLabel}`, {
        ...(canSwitchBack
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
          : {}),
      });
    },
    [activeModel, downloadedModels, setActiveChatModel, settings, updateProvider]
  );

  return { handleChangeSpace, handleSearchGeneral, handleSwitchActiveModel };
}
