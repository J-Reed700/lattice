import { useCallback, useEffect, useMemo } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { type UnlistenFn } from '@tauri-apps/api/event';

import { VaultAPI } from '../lib/api';
import { toast } from '../stores/toastStore';
import { TauriEventNames, EventSchemas, listenValidated } from '../types/events';

import type { DownloadedModel } from '../types/downloadedModels';

export const DOWNLOADED_MODELS_QUERY_KEY = ['downloaded-models'] as const;

async function loadDownloadedModels(): Promise<DownloadedModel[]> {
  const result = await VaultAPI.getDownloadedModels();
  if (result.ok) return result.data;
  // Compatibility fallback for older backend routing. The returned data still
  // enters the same React Query cache; it never becomes a second state owner.
  return invoke<DownloadedModel[]>('plugin:model|list_downloaded_models');
}

export const useDownloadedModels = () => {
  const queryClient = useQueryClient();
  const modelsQuery = useQuery<DownloadedModel[]>({
    queryKey: DOWNLOADED_MODELS_QUERY_KEY,
    queryFn: loadDownloadedModels,
    staleTime: 30_000,
  });
  const downloadedModels = useMemo(() => modelsQuery.data ?? [], [modelsQuery.data]);
  const activeModel = useMemo(
    () => downloadedModels.find((model) => model.is_active_for_chat) ?? null,
    [downloadedModels]
  );
  const activeEmbeddingModel = useMemo(
    () => downloadedModels.find((model) => model.is_active_for_embedding) ?? null,
    [downloadedModels]
  );
  const downloadedModelMap = useMemo(
    () => new Map(downloadedModels.map((model) => [model.id, model])),
    [downloadedModels]
  );

  const refresh = useCallback(async (): Promise<DownloadedModel[]> =>
    queryClient.fetchQuery({
      queryKey: DOWNLOADED_MODELS_QUERY_KEY,
      queryFn: loadDownloadedModels,
      staleTime: 0,
    }), [queryClient]);

  const invalidateModels = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: DOWNLOADED_MODELS_QUERY_KEY });
  }, [queryClient]);

  const setActiveChatMutation = useMutation({
    mutationFn: async (modelId: string) => {
      const result = await VaultAPI.setActiveChatModel(modelId);
      if (!result.ok) throw new Error(result.error);
    },
    onSuccess: invalidateModels,
  });
  const setActiveEmbeddingMutation = useMutation({
    mutationFn: async (modelId: string) => {
      const result = await VaultAPI.setActiveEmbeddingModel(modelId);
      if (!result.ok) throw new Error(result.error);
    },
    onSuccess: invalidateModels,
  });
  const setActiveUtilityMutation = useMutation({
    mutationFn: async (modelId: string) => {
      const result = await VaultAPI.setActiveUtilityModel(modelId);
      if (!result.ok) throw new Error(result.error);
    },
    onSuccess: invalidateModels,
  });
  const clearActiveUtilityMutation = useMutation({
    mutationFn: async () => {
      const result = await VaultAPI.clearActiveUtilityModel();
      if (!result.ok) throw new Error(result.error);
    },
    onSuccess: invalidateModels,
  });
  const deleteModelMutation = useMutation({
    mutationFn: async ({ id, deleteFile }: { id: string; deleteFile: boolean }) => {
      const result = await VaultAPI.deleteDownloadedModel(id, deleteFile);
      if (!result.ok) {
        await invoke<void>('plugin:model|delete_model', {
          modelId: id,

          deleteFile,

        });
      }
    },
    onSuccess: invalidateModels,
  });

  const isModelDownloaded = useCallback(async (modelId: string): Promise<boolean> => {
    const result = await VaultAPI.isModelDownloaded(modelId);
    if (result.ok) return result.data;
    try {
      return await invoke<boolean>('plugin:model|is_model_already_downloaded', {
        modelId,

      });
    } catch {
      return downloadedModels.some((model) => model.model_id === modelId);
    }
  }, [downloadedModels]);

  const warmUpActiveChatModel = useCallback(async () => {
    const result = await VaultAPI.warmUpActiveChatModel();
    if (!result.ok) throw new Error(result.error);
  }, []);
  const warmUpActiveUtilityModel = useCallback(async () => {
    const result = await VaultAPI.warmUpActiveUtilityModel();
    if (!result.ok) throw new Error(result.error);
  }, []);

  return {
    downloadedModels,
    downloadedModelMap,
    activeModel,
    activeEmbeddingModel,
    isLoading: modelsQuery.isLoading,
    error: modelsQuery.error?.message ?? null,
    fetchDownloadedModels: refresh,
    isModelDownloaded,
    setActiveChatModel: setActiveChatMutation.mutateAsync,
    warmUpActiveChatModel,
    warmUpActiveUtilityModel,
    setActiveEmbeddingModel: setActiveEmbeddingMutation.mutateAsync,
    setActiveUtilityModel: setActiveUtilityMutation.mutateAsync,
    clearActiveUtilityModel: clearActiveUtilityMutation.mutateAsync,
    deleteDownloadedModel: (id: string, deleteFile = true) =>
      deleteModelMutation.mutateAsync({ id, deleteFile }),
    getActiveModel: async () => (await refresh()).find((model) => model.is_active_for_chat) ?? null,
    getActiveEmbeddingModel: async () =>
      (await refresh()).find((model) => model.is_active_for_embedding) ?? null,
    getDownloadedModel: (id: string) => downloadedModels.find((model) => model.id === id),
    getAllDownloadedModels: () => downloadedModels,
  };
};

/** Own the model-completion listener at one app-level mount. */
export const useDownloadedModelsListener = () => {
  const { fetchDownloadedModels } = useDownloadedModels();

  useEffect(() => {
    let isMounted = true;
    let unlistenFn: UnlistenFn | undefined;

    void (async () => {
      try {
        const listener = await listenValidated(
          TauriEventNames.Downloads.Completed,
          EventSchemas.Downloads.Completed,
          async (event) => {
            const { modelName } = event.payload;
            try {
              const updated = await fetchDownloadedModels();
              toast.success(`${modelName} ready`);
              const completed = updated.find((model) => model.model_name === modelName);
              if (completed?.is_active_for_chat) {
                void VaultAPI.warmUpActiveChatModel().catch((error) =>
                  console.warn('[useDownloadedModels] post-download chat warmup failed:', error)
                );
              }
              if (completed?.is_active_for_utility) {
                void VaultAPI.warmUpActiveUtilityModel().catch((error) =>
                  console.warn('[useDownloadedModels] post-download utility warmup failed:', error)
                );
              }
            } catch (error) {
              console.error('[useDownloadedModels] Failed to refresh after completion:', error);
            }
          },
          (error) => console.error('[useDownloadedModels] Invalid completion event:', error.format())
        );
        if (!isMounted) listener();
        else unlistenFn = listener;
      } catch (error) {
        console.error('[useDownloadedModels] Failed to setup event listener:', error);
      }
    })();

    void fetchDownloadedModels().catch(() => {
      // Expected during first run when no local model exists yet.
    });
    return () => {
      isMounted = false;
      unlistenFn?.();
    };
  }, [fetchDownloadedModels]);
};
