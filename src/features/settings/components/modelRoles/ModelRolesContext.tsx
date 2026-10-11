/**
 * ModelRolesContext
 *
 * Single source of truth for role assignment state in the AI Models tab.
 * Every card in the grid (local cards + the Ollama meta-card) reads from
 * this context and calls `assignRole()` to mutate.
 *
 * Why a context: chat/utility/embedding assignments are shared state that
 * multiple sibling cards need to read. Prop-drilling from the tab root gets
 * messy; a context keeps the cards decoupled and the mutation path unified.
 */

import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';

import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { toast } from '@/stores/toastStore';
import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';

import type { RoleId } from './roleConfig';

interface ModelRolesContextValue {
  models: DownloadedModel[];
  localModels: DownloadedModel[];
  ollamaModels: DownloadedModel[];
  chatProvider: LLMSettings['provider'] | undefined;
  isLoading: boolean;
  /** Why the last refresh failed; null once one succeeds. */
  error: string | null;
  /** Refresh the model list from the backend. */
  refresh: () => Promise<void>;
  /** Assign `modelId` to the given role, or clear the role if modelId is null. */
  assignRole: (modelId: string | null, role: RoleId) => Promise<void>;
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
  const chatProvider = settings?.llm.provider;
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

  // Caught here, not by callers: the mount effect fires and forgets, so an
  // uncaught failure would be an unhandled rejection and an empty list that
  // claims no models are downloaded.
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
            // An explicit Ollama provider keeps working even without a model
            // row assignment. Deselecting it must also stop that provider.
            if (chatProvider === 'ollama') {
              await updateSettings({ category: 'llm', updates: { provider: 'local' } });
            }
          } else {
            const model = models.find(candidate => candidate.model_id === modelId);
            if (!model) throw new Error('Model is no longer available. Please refresh and try again.');
            await setActiveChatModel(modelId);
            const provider = model.backend === 'ollama' ? 'ollama' : 'local';
            // Setting a row flag alone cannot override an explicit remote
            // provider. Persist the user's choice through both selection paths.
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
        }
      } catch (error) {
        toast.error(`Failed to set ${role} role`, {
          message: error instanceof Error ? error.message : String(error),
        });
      } finally {
        // A provider write can fail after the model assignment succeeded.
        // Always re-read the persisted assignments, including that case.
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
      models,
      updateSettings,
    ],
  );

  const value = useMemo<ModelRolesContextValue>(() => {
    const localModels = models.filter((m) => (m.backend ?? 'local') === 'local');
    const ollamaModels = models.filter((m) => m.backend === 'ollama');
    return {
      models,
      localModels,
      ollamaModels,
      chatProvider,
      isLoading,
      error,
      refresh,
      assignRole,
    };
  }, [models, chatProvider, isLoading, error, refresh, assignRole]);

  return <ModelRolesContext.Provider value={value}>{children}</ModelRolesContext.Provider>;
}
