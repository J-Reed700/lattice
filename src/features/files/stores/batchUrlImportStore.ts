import { create } from 'zustand';

import type { UrlPreview } from '@/types/api/web';

export type ItemStatus = 'idle' | 'fetching' | 'ready' | 'importing' | 'success' | 'error';

export interface BatchImportItem {
  id: string;
  url: string;
  preview?: UrlPreview;
  status: ItemStatus;
  error?: string;
  selected: boolean;
}

export interface BatchImportOptions {
  extractArticle: boolean;
}

export interface BatchUrlImportResults {
  successful: number;
  failed: number;
}

interface BatchUrlImportState {
  urlText: string;
  items: BatchImportItem[];
  options: BatchImportOptions;
  isImporting: boolean;
  activeJobId: string | null;
  isCancelling: boolean;
  runError: string | null;
  progress: number;
  importStats: { success: number; error: number };
  lastRunResult: BatchUrlImportResults | null;

  setUrlTextOnly: (urlText: string) => void;
  setExtractArticle: (extractArticle: boolean) => void;
  toggleItem: (itemId: string) => void;
  toggleAll: () => void;
  removeItem: (itemId: string) => void;
  resetState: () => void;
}

export const useBatchUrlImportStore = create<BatchUrlImportState>((set) => ({
  urlText: '',
  items: [],
  options: { extractArticle: true },
  isImporting: false,
  activeJobId: null,
  isCancelling: false,
  runError: null,
  progress: 0,
  importStats: { success: 0, error: 0 },
  lastRunResult: null,

  setUrlTextOnly: (urlText) => set({ urlText }),

  setExtractArticle: (extractArticle) =>
    set((state) => ({
      options: { ...state.options, extractArticle },
    })),

  toggleItem: (itemId) =>
    set((state) => ({
      items: state.items.map((item) =>
        item.id === itemId ? { ...item, selected: !item.selected } : item,
      ),
    })),

  toggleAll: () =>
    set((state) => {
      const allSelected = state.items.every((item) => item.selected);
      return {
        items: state.items.map((item) => ({ ...item, selected: !allSelected })),
      };
    }),

  removeItem: (itemId) =>
    set((state) => ({
      items: state.items.filter((item) => item.id !== itemId),
    })),

  resetState: () =>
    set({
      urlText: '',
      items: [],
      options: { extractArticle: true },
      isImporting: false,
      activeJobId: null,
      isCancelling: false,
      runError: null,
      progress: 0,
      importStats: { success: 0, error: 0 },
      lastRunResult: null,
    }),
}));
