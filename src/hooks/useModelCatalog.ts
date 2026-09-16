import { useCallback, useMemo } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import { useModelCatalogStore } from '@/stores/modelCatalogStore';
import { toast } from '@/stores/toastStore';
import type {
  ModelCatalogCacheStats,
  ModelRecommendation,
  ModelSearchResult,
  SystemCapabilities,
} from '@/types';
import type { ModelCategory, SearchModelCatalogRequest } from '@/types/modelCatalog';

import { useDebounce } from './useDebounce';

const modelCatalogKeys = {
  all: ['model-catalog'] as const,
  capabilities: ['model-catalog', 'capabilities'] as const,
  compatible: (category?: ModelCategory) => ['model-catalog', 'compatible', category ?? 'all'] as const,
  models: ['model-catalog', 'models'] as const,
  search: (request: SearchModelCatalogRequest | null) => ['model-catalog', 'search', request] as const,
  cacheStats: ['model-catalog', 'cache-stats'] as const,
};

async function unwrap<T>(promise: Promise<{ ok: true; data: T } | { ok: false; error: string }>): Promise<T> {
  const result = await promise;
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export interface UseModelCatalogOptions {
  autoLoadCapabilities?: boolean;
  autoLoadModels?: boolean;
  category?: ModelCategory;
  searchDebounceMs?: number;
  loadCacheStats?: boolean;
}

/**
 * React Query mirror of the backend model catalog repository.
 * Zustand owns only filters, selection, and sort preferences.
 */
export function useModelCatalog(options: UseModelCatalogOptions = {}) {
  const {
    autoLoadCapabilities = true,
    autoLoadModels = true,
    category,
    searchDebounceMs = 300,
    loadCacheStats = false,
  } = options;
  const queryClient = useQueryClient();
  const {
    filters,
    searchQuery,
    selectedModel,
    sortBy,
    setFilters,
    setSearchQuery,
    setSortBy,
    selectModel,
    clearSelection,
  } = useModelCatalogStore();
  const debouncedSearchQuery = useDebounce(searchQuery, searchDebounceMs).trim();

  const capabilitiesQuery = useQuery<SystemCapabilities>({
    queryKey: modelCatalogKeys.capabilities,
    queryFn: () => unwrap(VaultAPI.detectSystemCapabilities()),
    enabled: autoLoadCapabilities,
    staleTime: 5 * 60_000,
  });

  const compatibleModelsQuery = useQuery<ModelRecommendation[]>({
    queryKey: modelCatalogKeys.compatible(category),
    queryFn: () => unwrap(category
      ? VaultAPI.getCompatibleModels(category)
      : VaultAPI.getAllRecommendedModels()),
    enabled: autoLoadModels,
    staleTime: 5 * 60_000,
  });

  const allModelsQuery = useQuery<ModelRecommendation[]>({
    queryKey: modelCatalogKeys.models,
    queryFn: () => unwrap(VaultAPI.getAllRecommendedModels()),
    enabled: false,
    staleTime: 5 * 60_000,
  });

  const searchRequest = useMemo<SearchModelCatalogRequest | null>(() => {
    if (!debouncedSearchQuery) return null;
    return {
      query: debouncedSearchQuery,
      category: filters.category,
      max_size_gb: filters.max_size_gb,
      required_capabilities: filters.required_capabilities,
      limit: 20,
    };
  }, [debouncedSearchQuery, filters.category, filters.max_size_gb, filters.required_capabilities]);

  const searchResultsQuery = useQuery<ModelSearchResult[]>({
    queryKey: modelCatalogKeys.search(searchRequest),
    queryFn: () => unwrap(VaultAPI.searchModelCatalog(searchRequest!)),
    enabled: searchRequest !== null,
    staleTime: 60_000,
  });

  const cacheStatsQuery = useQuery<ModelCatalogCacheStats>({
    queryKey: modelCatalogKeys.cacheStats,
    queryFn: () => unwrap(VaultAPI.getModelCatalogStats()),
    enabled: loadCacheStats,
    staleTime: 60_000,
  });

  const refreshMutation = useMutation({
    mutationFn: () => unwrap(VaultAPI.refreshModelCatalog()),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: modelCatalogKeys.all });
    },
    onError: (error: Error) => {
      console.error('[ModelCatalog] Refresh failed:', error);
      toast.warning('Model catalog may be outdated', {
        message: 'Using cached data. Try refreshing manually.',
        dismissible: true,
        duration: 5000,
      });
    },
  });

  const clearCacheMutation = useMutation({
    mutationFn: () => unwrap(VaultAPI.clearModelCatalogCache()),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: modelCatalogKeys.all });
    },
  });

  const loadSystemCapabilities = useCallback(async () => {
    await capabilitiesQuery.refetch();
  }, [capabilitiesQuery]);
  const loadCompatibleModels = useCallback(async (_category?: ModelCategory) => {
    await queryClient.fetchQuery({
      queryKey: modelCatalogKeys.compatible(_category),
      queryFn: () => unwrap(_category
        ? VaultAPI.getCompatibleModels(_category)
        : VaultAPI.getAllRecommendedModels()),
      staleTime: 5 * 60_000,
    });
  }, [queryClient]);
  const loadAllModels = useCallback(async () => {
    await allModelsQuery.refetch();
  }, [allModelsQuery]);
  const searchCatalog = useCallback(async (request: SearchModelCatalogRequest) => {
    await queryClient.fetchQuery({
      queryKey: modelCatalogKeys.search(request),
      queryFn: () => unwrap(VaultAPI.searchModelCatalog(request)),
      staleTime: 60_000,
    });
  }, [queryClient]);
  const loadCatalogCacheStats = useCallback(async () => {
    await cacheStatsQuery.refetch();
  }, [cacheStatsQuery]);
  const reloadSearch = useCallback(async () => {
    await searchResultsQuery.refetch({ throwOnError: true });
  }, [searchResultsQuery]);

  const capabilitiesError = capabilitiesQuery.error?.message ?? null;
  const compatibleModelsError = compatibleModelsQuery.error?.message ?? null;
  const allModelsError = allModelsQuery.error?.message ?? null;
  const searchError = searchResultsQuery.error?.message ?? null;
  const search = useCallback((query: string) => setSearchQuery(query), [setSearchQuery]);
  const debouncedSearch = useCallback((request: SearchModelCatalogRequest) => {
    void searchCatalog(request);
  }, [searchCatalog]);

  return {
    systemCapabilities: capabilitiesQuery.data ?? null,
    compatibleModels: compatibleModelsQuery.data ?? [],
    allModels: allModelsQuery.data ?? [],
    searchResults: searchResultsQuery.data ?? [],
    filters,
    searchQuery,
    selectedModel,
    sortBy,
    cacheStats: cacheStatsQuery.data ?? null,
    isLoading: capabilitiesQuery.isLoading || compatibleModelsQuery.isLoading ||
      allModelsQuery.isFetching || searchResultsQuery.isFetching,
    capabilitiesLoading: capabilitiesQuery.isFetching,
    compatibleModelsLoading: compatibleModelsQuery.isFetching,
    allModelsLoading: allModelsQuery.isFetching,
    searchLoading: searchResultsQuery.isFetching || searchQuery.trim() !== debouncedSearchQuery,
    capabilitiesError,
    compatibleModelsError,
    allModelsError,
    searchError,
    error: capabilitiesError || compatibleModelsError || allModelsError || searchError,
    isReady: capabilitiesQuery.data !== undefined && !capabilitiesQuery.isFetching,
    loadSystemCapabilities,
    loadCompatibleModels,
    loadAllModels,
    searchCatalog,
    reloadSearch,
    search,
    debouncedSearch,
    setFilters,
    setSearchQuery,
    setSortBy,
    selectModel,
    clearSelection,
    refreshCatalog: refreshMutation.mutateAsync,
    clearCache: clearCacheMutation.mutateAsync,
    loadCacheStats: loadCatalogCacheStats,
  };
}

export function useModelSearch(debounceMs = 300) {
  const { searchResults, searchLoading, searchError, search } = useModelCatalog({
    autoLoadCapabilities: false,
    autoLoadModels: false,
    searchDebounceMs: debounceMs,
  });
  return { searchResults, isSearching: searchLoading, searchError, search };
}

export function useCompatibleModels(category?: ModelCategory) {
  const { compatibleModels, compatibleModelsLoading, compatibleModelsError, loadCompatibleModels } =
    useModelCatalog({ category, autoLoadModels: true });
  const refresh = useCallback(() => {
    void loadCompatibleModels(category);
  }, [loadCompatibleModels, category]);
  return {
    models: category
      ? compatibleModels.filter((model) => model.model.category === category)
      : compatibleModels,
    isLoading: compatibleModelsLoading,
    error: compatibleModelsError,
    refresh,
  };
}
