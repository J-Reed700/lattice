import { useEffect, useCallback } from 'react';

import { type UnlistenFn } from '@tauri-apps/api/event';

import { VaultAPI } from '../lib/api';
import { useDownloadedModelsStore } from '../stores/downloadedModelsStore';
import { toast } from '../stores/toastStore';
import { TauriEventNames, EventSchemas, listenValidated } from '../types/events';

import type { DownloadedModel } from '../types/downloadedModels';

export const useDownloadedModels = () => {
  const {
    setDownloadedModel,
    removeDownloadedModel,
    setActiveModel,
    setActiveEmbeddingModel,
    getDownloadedModel,
    getAllDownloadedModels,
    isModelDownloaded: isModelDownloadedInStore,
  } = useDownloadedModelsStore();

  const fetchDownloadedModels = useCallback(async (): Promise<DownloadedModel[]> => {
    try {
      const result = await VaultAPI.getDownloadedModels();
      if (!result.ok) {
        console.debug('[useDownloadedModels] No models downloaded yet, returning empty array');
        return [];
      }

      result.data.forEach((model) => {
        setDownloadedModel(model.id, model);
      });

      return result.data;
    } catch {
      // Don't log error - empty model list is expected on first run
      console.debug('[useDownloadedModels] No models downloaded yet, returning empty array');
      return [];
    }
  }, [setDownloadedModel]);

  const isModelDownloaded = useCallback(async (model_id: string): Promise<boolean> => {
    const result = await VaultAPI.isModelDownloaded(model_id);
    if (!result.ok) {
      console.error('Failed to check model download status:', result.error);
      return isModelDownloadedInStore(model_id);
    }
    return result.data;
  }, [isModelDownloadedInStore]);

  const setActiveChatModel = useCallback(async (model_id: string): Promise<void> => {
    try {
      const result = await VaultAPI.setActiveChatModel(model_id);
      if (!result.ok) {
        throw new Error(result.error);
      }

      const updatedModels = await fetchDownloadedModels();
      const activeModel = updatedModels.find((m) => m.is_active_for_chat);
      setActiveModel(activeModel || null);
    } catch (error) {
      console.error('[useDownloadedModels] Failed to set active chat model:', error);
      throw new Error(`Failed to set active chat model: ${error instanceof Error ? error.message : String(error)}`);
    }
  }, [fetchDownloadedModels, setActiveModel]);

  const warmUpActiveChatModel = useCallback(async (): Promise<void> => {
    try {
      const result = await VaultAPI.warmUpActiveChatModel();
      if (!result.ok) {
        throw new Error(result.error);
      }
    } catch (error) {
      console.error('[useDownloadedModels] Failed to warm up active chat model:', error);
      throw new Error(`Failed to warm up active chat model: ${error instanceof Error ? error.message : String(error)}`);
    }
  }, []);

  const deleteDownloadedModel = useCallback(async (id: string, deleteFile = true): Promise<void> => {
    try {
      const result = await VaultAPI.deleteDownloadedModel(id, deleteFile);
      if (!result.ok) {
        throw new Error(result.error);
      }

      removeDownloadedModel(id);
    } catch (error) {
      console.error('[useDownloadedModels] Failed to delete model:', error);
      throw new Error(`Failed to delete model: ${error instanceof Error ? error.message : String(error)}`);
    }
  }, [removeDownloadedModel]);

  const getActiveModel = useCallback(async (): Promise<DownloadedModel | null> => {
    try {
      const result = await VaultAPI.getActiveModels();
      if (!result.ok) {
        throw new Error(result.error);
      }
      const { chat_model, embedding_model } = result.data;

      if (chat_model) {
        setActiveModel(chat_model);
      }

      if (embedding_model) {
        setActiveEmbeddingModel(embedding_model);
      }

      return chat_model || null;
    } catch (error) {
      console.error('[useDownloadedModels] Failed to get active models:', error);
      throw new Error(`Failed to get active chat model: ${error instanceof Error ? error.message : String(error)}`);
    }
  }, [setActiveModel, setActiveEmbeddingModel]);

  const setActiveEmbeddingModelById = useCallback(async (model_id: string): Promise<void> => {
    try {
      const result = await VaultAPI.setActiveEmbeddingModel(model_id);
      if (!result.ok) {
        throw new Error(result.error);
      }

      const updatedModels = await fetchDownloadedModels();
      const activeEmbModel = updatedModels.find((m) => m.is_active_for_embedding);
      setActiveEmbeddingModel(activeEmbModel || null);
    } catch (error) {
      console.error('[useDownloadedModels] Failed to set active embedding model:', error);
      throw new Error(`Failed to set active embedding model: ${error instanceof Error ? error.message : String(error)}`);
    }
  }, [fetchDownloadedModels, setActiveEmbeddingModel]);

  const getActiveEmbeddingModel = useCallback(async (): Promise<DownloadedModel | null> => {
    try {
      const result = await VaultAPI.getActiveModels();
      if (!result.ok) {
        throw new Error(result.error);
      }
      const { chat_model, embedding_model } = result.data;

      if (chat_model) {
        setActiveModel(chat_model);
      }

      if (embedding_model) {
        setActiveEmbeddingModel(embedding_model);
      }

      return embedding_model || null;
    } catch (error) {
      console.error('[useDownloadedModels] Failed to get active embedding model:', error);
      throw new Error(`Failed to get active embedding model: ${error instanceof Error ? error.message : String(error)}`);
    }
  }, [setActiveModel, setActiveEmbeddingModel]);

  useEffect(() => {
    const isMounted = { current: true };
    let unlistenFn: UnlistenFn | undefined;

    const setupListener = async () => {
      try {
        // Listen for model download completion events from backend
        const listener = await listenValidated(
          TauriEventNames.Models.DownloadCompleted,
          EventSchemas.Models.DownloadCompleted,
          async (event) => {
          const { modelName } = event.payload;
          console.log('[useDownloadedModels] Model download completed:', modelName);

          try {
            // Refetch downloaded models to sync with database
            await fetchDownloadedModels();

            // Show success notification
            toast.success(`${modelName} ready`);
          } catch (error) {
            console.error('[useDownloadedModels] Failed to refresh after completion:', error);
          }
        },
          (error) => {
            console.error('[useDownloadedModels] Validation error for model-download-completed:', error.format());
          }
        );

        // Check if component unmounted during async operation
        if (!isMounted.current) {
          listener(); // Clean up immediately if unmounted
          return;
        }

        unlistenFn = listener;
      } catch (error) {
        console.error('[useDownloadedModels] Failed to setup event listener:', error);
      }
    };

    const loadModels = async () => {
      try {
        // fetchDownloadedModels already fetches all models with active flags
        // The store automatically updates activeModel and activeEmbeddingModel
        await fetchDownloadedModels();
        await getActiveModel();
      } catch {
        // Silently handle error - fetchDownloadedModels now returns empty array on error
        // This is expected on first run when no models are downloaded yet
      }
    };

    setupListener();
    loadModels();

    // Cleanup: unlisten when component unmounts
    return () => {
      isMounted.current = false;
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [fetchDownloadedModels, getActiveModel]);

  return {
    fetchDownloadedModels,
    isModelDownloaded,
    setActiveChatModel,
    warmUpActiveChatModel,
    setActiveEmbeddingModel: setActiveEmbeddingModelById,
    deleteDownloadedModel,
    getActiveModel,
    getActiveEmbeddingModel,
    getDownloadedModel,
    getAllDownloadedModels,
  };
};
