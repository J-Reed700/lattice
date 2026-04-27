/**
 * Model Catalog Store - Zustand Implementation
 *
 * Manages model catalog state including:
 * - System capabilities detection
 * - Compatible model recommendations
 * - Catalog search with external models
 * - Cache management
 */

import { create } from 'zustand';

import VaultAPI from '@/lib/api';
import type {
  ModelCatalogCacheStats,
  ModelRecommendation,
  ModelSearchResult,
  SystemCapabilities,
} from '@/types';

import { toast } from './toastStore';

import type {
  SearchFilters,
  ModelCategory,
  ModelSortBy,
  SearchModelCatalogRequest,
} from '../types/modelCatalog';

// ============================================================================
// State Definition
// ============================================================================

interface ModelCatalogState {
  // System capabilities
  systemCapabilities: SystemCapabilities | null;
  capabilitiesLoading: boolean;
  capabilitiesError: string | null;

  // Compatible models
  compatibleModels: ModelRecommendation[];
  compatibleModelsLoading: boolean;
  compatibleModelsError: string | null;

  // All models (for browsing)
  allModels: ModelRecommendation[];
  allModelsLoading: boolean;
  allModelsError: string | null;

  // Search results
  searchResults: ModelSearchResult[];
  searchLoading: boolean;
  searchError: string | null;

  // Filters and query
  filters: SearchFilters;
  searchQuery: string;
  sortBy: ModelSortBy;

  // Selected model
  selectedModel: ModelRecommendation | null;

  // Cache stats
  cacheStats: ModelCatalogCacheStats | null;
  cacheStatsLoading: boolean;
  cacheStatsError: string | null;
}

interface ModelCatalogActions {
  // Load system capabilities
  loadSystemCapabilities: () => Promise<void>;

  // Load compatible models
  loadCompatibleModels: (_category?: ModelCategory) => Promise<void>;

  // Load all models
  loadAllModels: () => Promise<void>;

  // Search catalog (curated + external)
  searchCatalog: (_request: SearchModelCatalogRequest) => Promise<void>;

  // Filter and query management
  setFilters: (_filters: Partial<SearchFilters>) => void;
  setSearchQuery: (_query: string) => void;
  setSortBy: (_sortBy: ModelSortBy) => void;

  // Selection
  selectModel: (_model: ModelRecommendation) => void;
  clearSelection: () => void;

  // Cache management
  refreshCatalog: () => Promise<void>;
  clearCache: () => Promise<void>;
  loadCacheStats: () => Promise<void>;
}

// ============================================================================
// Store Implementation
// ============================================================================

// Track request IDs for race condition prevention
let searchRequestId = 0;
let compatibleModelsRequestId = 0;
let allModelsRequestId = 0;

export const useModelCatalogStore = create<ModelCatalogState & ModelCatalogActions>((set, get) => ({
  // Initial state
  systemCapabilities: null,
  capabilitiesLoading: false,
  capabilitiesError: null,

  compatibleModels: [],
  compatibleModelsLoading: false,
  compatibleModelsError: null,

  allModels: [],
  allModelsLoading: false,
  allModelsError: null,

  searchResults: [],
  searchLoading: false,
  searchError: null,

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

  cacheStats: null,
  cacheStatsLoading: false,
  cacheStatsError: null,

  // Actions

  loadSystemCapabilities: async () => {
    set({ capabilitiesLoading: true, capabilitiesError: null });

    const result = await VaultAPI.detectSystemCapabilities();
    if (result.ok) {
      set({
        systemCapabilities: result.data,
        capabilitiesLoading: false,
        capabilitiesError: null,
      });
    } else {
      set({
        capabilitiesError: result.error,
        capabilitiesLoading: false,
      });
    }
  },

  loadCompatibleModels: async (_category?: ModelCategory) => {
    const currentRequestId = ++compatibleModelsRequestId;
    set({ compatibleModelsLoading: true, compatibleModelsError: null });

    console.log('[ModelCatalogStore] loadCompatibleModels called, category:', _category);

    const result = _category
      ? await VaultAPI.getCompatibleModels(_category)
      : await VaultAPI.getAllRecommendedModels();

    console.log('[ModelCatalogStore] Command result:', result);

    // Only update state if this is still the current request
    if (currentRequestId !== compatibleModelsRequestId) {
      return;
    }

    if (result.ok) {
      console.log('[ModelCatalogStore] Success! Got', result.data.length, 'models');
      set({
        compatibleModels: result.data,
        compatibleModelsLoading: false,
        compatibleModelsError: null,
      });
    } else {
      console.error('[ModelCatalogStore] Error:', result.error);
      const errorMessage = result.error;
      console.error('[ModelCatalogStore] Error message:', errorMessage);
      set({
        compatibleModelsError: errorMessage,
        compatibleModelsLoading: false,
      });
    }
  },

  loadAllModels: async () => {
    const currentRequestId = ++allModelsRequestId;
    set({ allModelsLoading: true, allModelsError: null });

    const result = await VaultAPI.getAllRecommendedModels();

    // Only update state if this is still the current request
    if (currentRequestId !== allModelsRequestId) {
      return;
    }

    if (result.ok) {
      set({
        allModels: result.data,
        allModelsLoading: false,
        allModelsError: null,
      });
    } else {
      set({
        allModelsError: result.error,
        allModelsLoading: false,
      });
    }
  },

  searchCatalog: async (_request: SearchModelCatalogRequest) => {
    const currentRequestId = ++searchRequestId;
    set({ searchLoading: true, searchError: null });

    const result = await VaultAPI.searchModelCatalog(_request);

    // Only update state if this is still the current request
    if (currentRequestId !== searchRequestId) {
      return;
    }

    if (result.ok) {
      set({
        searchResults: result.data,
        searchLoading: false,
        searchError: null,
      });
    } else {
      set({
        searchError: result.error,
        searchLoading: false,
      });
    }
  },

  setFilters: (_newFilters: Partial<SearchFilters>) => {
    set((state) => ({
      filters: {
        ...state.filters,
        ..._newFilters,
      },
    }));
  },

  setSearchQuery: (_query: string) => {
    set({ searchQuery: _query });
  },

  setSortBy: (_sortBy: ModelSortBy) => {
    set({ sortBy: _sortBy });
  },

  selectModel: (_model: ModelRecommendation) => {
    set({ selectedModel: _model });
  },

  clearSelection: () => {
    set({ selectedModel: null });
  },

  refreshCatalog: async () => {
    const result = await VaultAPI.refreshModelCatalog();

    if (!result.ok) {
      const errorMsg = result.error;
      console.error('[ModelCatalog] Refresh failed:', errorMsg);

      // Show dismissible toast for user awareness
      toast.warning('Model catalog may be outdated', {
        message: 'Using cached data. Try refreshing manually.',
        dismissible: true,
        duration: 5000
      });
      return;
    }

    // Success: reload data
    await get().loadCompatibleModels();
    // Clear search results to force re-search
    set({ searchResults: [] });
  },

  clearCache: async () => {
    const result = await VaultAPI.clearModelCatalogCache();
    if (result.ok) {
      console.log(`Cache cleared successfully: ${result.data} entries removed`);
      // Reload cache stats
      await get().loadCacheStats();
      // Clear search results
      set({ searchResults: [] });
    } else {
      console.error('Failed to clear cache:', result.error);
    }
  },

  loadCacheStats: async () => {
    set({ cacheStatsLoading: true, cacheStatsError: null });

    const result = await VaultAPI.getModelCatalogStats();
    if (result.ok) {
      set({
        cacheStats: result.data,
        cacheStatsLoading: false,
        cacheStatsError: null,
      });
    } else {
      set({
        cacheStatsError: result.error,
        cacheStatsLoading: false,
      });
    }
  },
}));

// ============================================================================
// Selectors
// ============================================================================

export const selectSystemCapabilities = (state: ModelCatalogState & ModelCatalogActions) =>
  state.systemCapabilities;

export const selectCapabilitiesLoading = (state: ModelCatalogState & ModelCatalogActions) =>
  state.capabilitiesLoading;

export const selectCompatibleModels = (state: ModelCatalogState & ModelCatalogActions) =>
  state.compatibleModels;

export const selectCompatibleModelsLoading = (state: ModelCatalogState & ModelCatalogActions) =>
  state.compatibleModelsLoading;

export const selectAllModels = (state: ModelCatalogState & ModelCatalogActions) =>
  state.allModels;

export const selectSearchResults = (state: ModelCatalogState & ModelCatalogActions) =>
  state.searchResults;

export const selectSearchLoading = (state: ModelCatalogState & ModelCatalogActions) =>
  state.searchLoading;

export const selectFilters = (state: ModelCatalogState & ModelCatalogActions) =>
  state.filters;

export const selectSearchQuery = (state: ModelCatalogState & ModelCatalogActions) =>
  state.searchQuery;

export const selectSelectedModel = (state: ModelCatalogState & ModelCatalogActions) =>
  state.selectedModel;

export const selectCacheStats = (state: ModelCatalogState & ModelCatalogActions) =>
  state.cacheStats;

// Computed selectors

export const selectIsReady = (state: ModelCatalogState & ModelCatalogActions) =>
  state.systemCapabilities !== null && !state.capabilitiesLoading;

export const selectHasCompatibleModels = (state: ModelCatalogState & ModelCatalogActions) =>
  state.compatibleModels.length > 0;

export const selectExcellentModels = (state: ModelCatalogState & ModelCatalogActions) =>
  state.compatibleModels.filter((m) => m.compatibility.compatibility_level === 'Excellent');

export const selectGoodModels = (state: ModelCatalogState & ModelCatalogActions) =>
  state.compatibleModels.filter(
    (m) => m.compatibility.compatibility_level === 'Good' ||
           m.compatibility.compatibility_level === 'Excellent'
  );

export const selectModelsByCategory = (
  state: ModelCatalogState & ModelCatalogActions,
  category: ModelCategory
) => state.compatibleModels.filter((m) => m.model.category === category);
