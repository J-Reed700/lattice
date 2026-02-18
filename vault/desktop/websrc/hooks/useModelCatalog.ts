/**
 * useModelCatalog Hook
 *
 * Custom hook for interacting with the model catalog store.
 * Provides:
 * - Auto-loading of system capabilities and compatible models on mount
 * - Debounced search functionality
 * - All store state and actions
 * - Cleanup on unmount
 */

import { useEffect, useCallback } from 'react';

import { useDebounce } from './useDebounce';
import { useModelCatalogStore } from '../stores/modelCatalogStore';

import type { SearchModelCatalogRequest, ModelCategory } from '../types/modelCatalog';

export interface UseModelCatalogOptions {
  /** Auto-load system capabilities on mount (default: true) */
  autoLoadCapabilities?: boolean;
  /** Auto-load compatible models on mount (default: true) */
  autoLoadModels?: boolean;
  /** Category filter for auto-loaded models */
  category?: ModelCategory;
  /** Debounce delay for search in milliseconds (default: 300) */
  searchDebounceMs?: number;
}

/**
 * Hook for interacting with the model catalog.
 *
 * @example
 * ```tsx
 * function ModelBrowser() {
 *   const {
 *     systemCapabilities,
 *     compatibleModels,
 *     isLoading,
 *     search,
 *     selectModel,
 *   } = useModelCatalog();
 *
 *   if (isLoading) return <div>Loading...</div>;
 *
 *   return (
 *     <div>
 *       <SearchInput onChange={search} />
 *       <ModelList models={compatibleModels} onSelect={selectModel} />
 *     </div>
 *   );
 * }
 * ```
 */
export function useModelCatalog(options: UseModelCatalogOptions = {}) {
  const {
    autoLoadCapabilities = true,
    autoLoadModels = true,
    category,
    searchDebounceMs = 300,
  } = options;

  // Get all state and actions from store
  const {
    systemCapabilities,
    capabilitiesLoading,
    capabilitiesError,
    compatibleModels,
    compatibleModelsLoading,
    compatibleModelsError,
    allModels,
    allModelsLoading,
    allModelsError,
    searchResults,
    searchLoading,
    searchError,
    filters,
    searchQuery,
    selectedModel,
    cacheStats,
    loadSystemCapabilities,
    loadCompatibleModels,
    loadAllModels,
    searchCatalog,
    setFilters,
    setSearchQuery,
    selectModel,
    clearSelection,
    refreshCatalog,
    clearCache,
    loadCacheStats,
  } = useModelCatalogStore();

  // Debounce search query for catalog search
  const debouncedSearchQuery = useDebounce(searchQuery, searchDebounceMs);

  // Load system capabilities on mount
  useEffect(() => {
    if (autoLoadCapabilities && !systemCapabilities && !capabilitiesLoading) {
      void loadSystemCapabilities();
    }
  }, [autoLoadCapabilities, systemCapabilities, capabilitiesLoading, loadSystemCapabilities]);

  // Load compatible models on mount (after capabilities are loaded)
  useEffect(() => {
    if (
      autoLoadModels &&
      systemCapabilities &&
      !capabilitiesLoading &&
      compatibleModels.length === 0 &&
      !compatibleModelsLoading
    ) {
      void loadCompatibleModels(category);
    }
  }, [
    autoLoadModels,
    systemCapabilities,
    capabilitiesLoading,
    compatibleModels.length,
    compatibleModelsLoading,
    category,
    loadCompatibleModels,
  ]);

  // Perform search when debounced query changes
  useEffect(() => {
    if (debouncedSearchQuery.trim()) {
      const request: SearchModelCatalogRequest = {
        query: debouncedSearchQuery,
        category: filters.category || null,
        max_size_gb: filters.max_size_gb ?? null,
        required_capabilities: filters.required_capabilities,
        limit: 20,
      };
      void searchCatalog(request);
    }
  }, [debouncedSearchQuery, filters, searchCatalog]);

  // Memoized search function
  const search = useCallback(
    (query: string) => {
      setSearchQuery(query);
    },
    [setSearchQuery]
  );

  // Memoized debounced search function for external use
  const debouncedSearch = useCallback(
    (request: SearchModelCatalogRequest) => {
      void searchCatalog(request);
    },
    [searchCatalog]
  );

  // Combined loading state
  const isLoading = capabilitiesLoading || compatibleModelsLoading || allModelsLoading || searchLoading;

  // Combined error state
  const error = capabilitiesError || compatibleModelsError || allModelsError || searchError;

  // Check if ready (capabilities loaded)
  const isReady = systemCapabilities !== null && !capabilitiesLoading;

  return {
    // State
    systemCapabilities,
    compatibleModels,
    allModels,
    searchResults,
    filters,
    searchQuery,
    selectedModel,
    cacheStats,

    // Loading states
    isLoading,
    capabilitiesLoading,
    compatibleModelsLoading,
    allModelsLoading,
    searchLoading,

    // Error states
    error,
    capabilitiesError,
    compatibleModelsError,
    allModelsError,
    searchError,

    // Computed
    isReady,

    // Actions
    loadSystemCapabilities,
    loadCompatibleModels,
    loadAllModels,
    search,
    debouncedSearch,
    setFilters,
    selectModel,
    clearSelection,
    refreshCatalog,
    clearCache,
    loadCacheStats,
  };
}

/**
 * Hook for searching the model catalog with automatic debouncing.
 *
 * @example
 * ```tsx
 * function ModelSearch() {
 *   const { searchResults, isSearching, search } = useModelSearch();
 *
 *   return (
 *     <div>
 *       <input onChange={(e) => search(e.target.value)} />
 *       {isSearching && <Spinner />}
 *       <Results results={searchResults} />
 *     </div>
 *   );
 * }
 * ```
 */
export function useModelSearch(debounceMs = 300) {
  const {
    searchResults,
    searchLoading,
    searchError,
    search,
  } = useModelCatalog({ searchDebounceMs: debounceMs });

  return {
    searchResults,
    isSearching: searchLoading,
    searchError,
    search,
  };
}

/**
 * Hook for managing compatible models by category.
 *
 * @example
 * ```tsx
 * function LLMModelSelector() {
 *   const { models, isLoading, refresh } = useCompatibleModels('LLM');
 *
 *   return (
 *     <div>
 *       <button onClick={refresh}>Refresh</button>
 *       <ModelGrid models={models} />
 *     </div>
 *   );
 * }
 * ```
 */
export function useCompatibleModels(category?: ModelCategory) {
  const {
    compatibleModels,
    compatibleModelsLoading,
    compatibleModelsError,
    loadCompatibleModels,
  } = useModelCatalog({ category, autoLoadModels: true });

  const refresh = useCallback(() => {
    void loadCompatibleModels(category);
  }, [loadCompatibleModels, category]);

  // Filter by category if specified
  const filteredModels = category
    ? compatibleModels.filter((m) => m.model.category === category)
    : compatibleModels;

  return {
    models: filteredModels,
    isLoading: compatibleModelsLoading,
    error: compatibleModelsError,
    refresh,
  };
}
