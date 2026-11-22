import { create } from 'zustand';

interface BatchFileImportState {
  selectedCollectionId: string;
  setSelectedCollectionId: (collectionId: string) => void;
  reset: () => void;
}

export const useBatchFileImportStore = create<BatchFileImportState>((set) => ({
  selectedCollectionId: '',
  setSelectedCollectionId: (selectedCollectionId) => set({ selectedCollectionId }),
  reset: () => set({ selectedCollectionId: '' }),
}));
