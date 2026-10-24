import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';

import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { ModelRef } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';
import { modelRefForRow } from '@/utils/chatModelSelection';

import type { RoleId } from './roleConfig';

interface ModelRolesContextValue {
  models: DownloadedModel[];
  localModels: DownloadedModel[];
  ollamaModels: DownloadedModel[];
  chatProvider: LLMSettings['provider'] | undefined;
  llmSettings: LLMSettings | undefined;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  assignRole: (modelId: string | null, role: RoleId) => Promise<void>;
  assignUtilityReference: (modelRef: ModelRef | null) => Promise<void>;
}

const ModelRolesContext = createContext<ModelRolesContextValue | null>(null);

export function useModelRoles(): ModelRolesContextValue {
  const ctx = useContext(ModelRolesContext);
  if (!ctx) {
    throw new Error('useModelRoles must be used inside <ModelRolesProvider>');
  }
  return ctx;
}

interface ProviderProps {
  children: ReactNode;
}

export function ModelRolesProvider({ children }: ProviderProps) {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  const llmSettings = settings?.llm;
  const chatProvider = llmSettings?.provider;
  const {
    fetchDownloadedModels,
    setActiveChatModel,
    setActiveEmbeddingModel,
    setActiveUtilityModel,
    clearActiveChatModel,
    clearActiveEmbeddingModel,
    clearActiveUtilityModel,
  } = useDownloadedModels();

  const [models, setModels] = useState<DownloadedModel[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      const all = await fetchDownloadedModels();
      setModels(all);
      setError(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setIsLoading(false);
    }
  }, [fetchDownloadedModels]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const assignRole = useCallback(
    async (modelId: string | null, role: RoleId) => {
      try {
        if (role === 'chat') {
          if (!chatProvider) throw new Error('Chat settings have not loaded. Please try again.');
          if (modelId === null) {
            await clearActiveChatModel();
            if (chatProvider === 'ollama') {
              await updateSettings({ category: 'llm', updates: { provider: 'local' } });
            }
          } else {
            const model = models.find(candidate => candidate.model_id === modelId);
            if (!model) throw new Error('Model is no longer available. Please refresh and try again.');
            await setActiveChatModel(modelId);
            const provider = model.backend === 'ollama' ? 'ollama' : 'local';
            if (chatProvider !== provider) {
              await updateSettings({ category: 'llm', updates: { provider } });
            }
          }
        } else if (role === 'embedding') {
          if (modelId === null) {
            await clearActiveEmbeddingModel();
          } else {
            await setActiveEmbeddingModel(modelId);
            toast.info('Embedding model prepared. Restart Lattice to use its search index.');
          }
        } else if (role === 'utility') {
          if (modelId === null) {
            await clearActiveUtilityModel();
          } else {
            await setActiveUtilityModel(modelId);
          }
          if (llmSettings?.utilityModel) {
            await updateSettings({ category: 'llm', updates: { utilityModel: null } });
          }
        } else if (role === 'learning' || role === 'verifier') {
          let modelRef = null;
          if (modelId !== null) {
            const model = models.find(candidate => candidate.model_id === modelId);
            if (!model) throw new Error('Model is no longer available. Please refresh and try again.');
            modelRef = modelRefForRow(model, llmSettings);
            if (!modelRef) throw new Error('Set the Ollama chat tag in Chat settings first.');
          }
          const key = role === 'learning' ? 'learningModel' : 'verifierModel';
          await updateSettings({ category: 'llm', updates: { [key]: modelRef } });
        }
      } catch (error) {
        toast.error(`Failed to set ${role} role`, {
          message: error instanceof Error ? error.message : String(error),
        });
      } finally {
        await refresh();
      }
    },
    [
      setActiveChatModel,
      setActiveEmbeddingModel,
      setActiveUtilityModel,
      clearActiveChatModel,
      clearActiveEmbeddingModel,
      clearActiveUtilityModel,
      refresh,
      chatProvider,
      llmSettings,
      models,
      updateSettings,
    ],
  );

  const assignUtilityReference = useCallback(
    async (modelRef: ModelRef | null) => {
      try {
        await updateSettings({ category: 'llm', updates: { utilityModel: modelRef } });
        if (modelRef) await clearActiveUtilityModel();
      } catch (error) {
        toast.error('Failed to set utility role', {
          message: error instanceof Error ? error.message : String(error),
        });
      } finally {
        await refresh();
      }
    },
    [clearActiveUtilityModel, refresh, updateSettings],
  );

  const value = useMemo<ModelRolesContextValue>(() => {
    const localModels = models.filter((m) => (m.backend ?? 'local') === 'local');
    const ollamaModels = models.filter((m) => m.backend === 'ollama');
    return {
      models,
      localModels,
      ollamaModels,
      chatProvider,
      llmSettings,
      isLoading,
      error,
      refresh,
      assignRole,
      assignUtilityReference,
    };
  }, [models, chatProvider, llmSettings, isLoading, error, refresh, assignRole, assignUtilityReference]);

  return <ModelRolesContext.Provider value={value}>{children}</ModelRolesContext.Provider>;
}
