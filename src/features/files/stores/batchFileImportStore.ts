import { create } from 'zustand';

interface BatchFileImportState {
  selectedCollectionId: string;
  jobCollections: Record<string, string>;
  setSelectedCollectionId: (collectionId: string) => void;
  setJobCollection: (jobId: string, collectionId: string) => void;
  removeJobCollection: (jobId: string) => void;
  reset: () => void;
}

export const useBatchFileImportStore = create<BatchFileImportState>((set) => ({
  selectedCollectionId: '',
  jobCollections: {},
  setSelectedCollectionId: (selectedCollectionId) => set({ selectedCollectionId }),
  setJobCollection: (jobId, collectionId) =>
    set((state) => ({
      jobCollections: { ...state.jobCollections, [jobId]: collectionId },
    })),
  removeJobCollection: (jobId) =>
    set((state) => {
      if (!(jobId in state.jobCollections)) return state;
      const next = { ...state.jobCollections };
      delete next[jobId];
      return { jobCollections: next };
    }),
  reset: () => set({ selectedCollectionId: '', jobCollections: {} }),
}));
