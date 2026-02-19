import { create } from 'zustand';

import type { DownloadedModel } from '../types/downloadedModels';

interface DownloadedModelsStore {
  downloadedModels: Map<string, DownloadedModel>;
  activeModel: DownloadedModel | null;
  activeEmbeddingModel: DownloadedModel | null;

  setDownloadedModel: (id: string, model: DownloadedModel) => void;
  removeDownloadedModel: (id: string) => void;
  setActiveModel: (model: DownloadedModel | null) => void;
  setActiveEmbeddingModel: (model: DownloadedModel | null) => void;
  clearDownloadedModels: () => void;

  getDownloadedModel: (id: string) => DownloadedModel | undefined;
  getAllDownloadedModels: () => DownloadedModel[];
  isModelDownloaded: (model_id: string) => boolean;
}

export const useDownloadedModelsStore = create<DownloadedModelsStore>((set, get) => ({
  downloadedModels: new Map(),
  activeModel: null,
  activeEmbeddingModel: null,

  setDownloadedModel: (id, model) => {
    set((state) => {
      const newDownloadedModels = new Map(state.downloadedModels);
      newDownloadedModels.set(id, model);

      let newActiveModel = state.activeModel;
      if (model.is_active_for_chat) {
        newActiveModel = model;
      }

      let newActiveEmbeddingModel = state.activeEmbeddingModel;
      if (model.is_active_for_embedding) {
        newActiveEmbeddingModel = model;
      }

      return {
        downloadedModels: newDownloadedModels,
        activeModel: newActiveModel,
        activeEmbeddingModel: newActiveEmbeddingModel,
      };
    });
  },

  removeDownloadedModel: (id) => {
    set((state) => {
      const newDownloadedModels = new Map(state.downloadedModels);
      const modelToRemove = newDownloadedModels.get(id);
      newDownloadedModels.delete(id);

      let newActiveModel = state.activeModel;
      if (modelToRemove?.is_active_for_chat) {
        newActiveModel = null;
      }

      let newActiveEmbeddingModel = state.activeEmbeddingModel;
      if (modelToRemove?.is_active_for_embedding) {
        newActiveEmbeddingModel = null;
      }

      return {
        downloadedModels: newDownloadedModels,
        activeModel: newActiveModel,
        activeEmbeddingModel: newActiveEmbeddingModel,
      };
    });
  },

  setActiveModel: (model) => {
    set({ activeModel: model });
  },

  setActiveEmbeddingModel: (model) => {
    set({ activeEmbeddingModel: model });
  },

  clearDownloadedModels: () => {
    set({ downloadedModels: new Map(), activeModel: null, activeEmbeddingModel: null });
  },

  getDownloadedModel: (id) => get().downloadedModels.get(id),

  getAllDownloadedModels: () => Array.from(get().downloadedModels.values()),

  isModelDownloaded: (model_id) => {
    const models = Array.from(get().downloadedModels.values());
    return models.some((model) => model.model_id === model_id);
  },
}));
