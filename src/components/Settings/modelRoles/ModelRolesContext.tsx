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

import { invoke } from '@tauri-apps/api/core';

import { useDownloadedModels } from '../../../hooks/useDownloadedModels';
import { toast } from '../../../stores/toastStore';

import type { RoleId } from './roleConfig';
import type { DownloadedModel } from '../../../types/downloadedModels';

interface ModelRolesContextValue {
  models: DownloadedModel[];
  localModels: DownloadedModel[];
  ollamaModels: DownloadedModel[];
  isLoading: boolean;
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
  const {
    fetchDownloadedModels,
    setActiveChatModel,
    setActiveEmbeddingModel,
    setActiveUtilityModel,
    clearActiveUtilityModel,
  } = useDownloadedModels();

  const [models, setModels] = useState<DownloadedModel[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      const all = await fetchDownloadedModels();
      setModels(all);
    } finally {
      setIsLoading(false);
    }
  }, [fetchDownloadedModels]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const assignRole = useCallback(
    async (modelId: string | null, role: RoleId) => {
      try {
        if (role === 'chat') {
          if (modelId === null) {
            await invoke('plugin:model|clear_active_chat_model');
          } else {
            await setActiveChatModel(modelId);
          }
        } else if (role === 'embedding') {
          if (modelId === null) {
            await invoke('plugin:model|clear_active_embedding_model');
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
        await refresh();
      } catch (error) {
        toast.error(`Failed to set ${role} role`, {
          message: error instanceof Error ? error.message : String(error),
        });
      }
    },
    [
      setActiveChatModel,
      setActiveEmbeddingModel,
      setActiveUtilityModel,
      clearActiveUtilityModel,
      refresh,
    ],
  );

  const value = useMemo<ModelRolesContextValue>(() => {
    const localModels = models.filter((m) => (m.backend ?? 'local') === 'local');
    const ollamaModels = models.filter((m) => m.backend === 'ollama');
    return {
      models,
      localModels,
      ollamaModels,
      isLoading,
      refresh,
      assignRole,
    };
  }, [models, isLoading, refresh, assignRole]);

  return <ModelRolesContext.Provider value={value}>{children}</ModelRolesContext.Provider>;
}
