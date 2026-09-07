import { create } from 'zustand';

import type { ModelRecommendation } from '@/types';
import type { ModelSortBy, SearchFilters } from '@/types/modelCatalog';

/**
 * UI-only model catalog preferences.
 *
 * Catalog records, capabilities, search results, and cache statistics are
 * backend-owned and live in React Query (see useModelCatalog). Keeping only
 * view preferences here preserves the repository as the single source of truth.
 */
interface ModelCatalogUiState {
  filters: SearchFilters;
  searchQuery: string;
  sortBy: ModelSortBy;
  selectedModel: ModelRecommendation | null;
  setFilters: (filters: Partial<SearchFilters>) => void;
  setSearchQuery: (query: string) => void;
  setSortBy: (sortBy: ModelSortBy) => void;
  selectModel: (model: ModelRecommendation) => void;
  clearSelection: () => void;
}

export const useModelCatalogStore = create<ModelCatalogUiState>((set) => ({
  filters: {
    category: null,
    max_size_gb: null,
    min_downloads: null,
    required_capabilities: [],
    query_text: null,
    embedding_dimensions: null,
  },
  searchQuery: '',
  sortBy: 'popularity',
  selectedModel: null,
  setFilters: (filters) => set((state) => ({ filters: { ...state.filters, ...filters } })),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setSortBy: (sortBy) => set({ sortBy }),
  selectModel: (selectedModel) => set({ selectedModel }),
  clearSelection: () => set({ selectedModel: null }),
}));
