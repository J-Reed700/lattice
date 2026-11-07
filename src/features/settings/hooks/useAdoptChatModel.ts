import { useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { toast } from '@/stores/toastStore';
import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';
import { resolveDefaultChatOption } from '@/utils/chatModelSelection';

/**
 * The chat model Learning may be set to, and the one write that sets it.
 * Learning never borrows the chat model by itself; this is the explicit click.
 */
export function useAdoptChatModel(settings: LLMSettings | undefined, models: DownloadedModel[]) {
  const { mutateAsync: updateSettings, isPending } = useUpdateSettingsMutation();
  const chatModel = settings ? resolveDefaultChatOption(settings, models) : null;
  const adopt = async () => {
    if (!chatModel) return;
    try {
      await updateSettings({ category: 'llm', updates: { learningModel: chatModel.ref } });
    } catch (error) {
      toast.error('Failed to set learning role', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };
  return { chatModel, adopt, isPending };
}
