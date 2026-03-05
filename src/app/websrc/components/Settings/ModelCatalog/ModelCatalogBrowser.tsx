/**
 * ModelCatalogBrowser
 *
 * Main container for the model catalog feature
 * Coordinates between list view, detail view, filters, and search
 */

import { useEffect, useMemo } from 'react';

import { CatalogManagementSection } from './CatalogManagementSection';
import { ModelDetailPanel } from './ModelDetailPanel';
import { ModelFilterPanel } from './ModelFilterPanel';
import { ModelListView } from './ModelListView';
import { ModelSearchBar } from './ModelSearchBar';
import { SystemCapabilitiesCard } from './SystemCapabilitiesCard';
import { useModelCatalogStore } from '../../../stores/modelCatalogStore';

interface ModelCatalogBrowserProps {
  routerModelId?: string;
  onSetRouterModel?: (_modelId: string) => Promise<void>;
}

export function ModelCatalogBrowser({ routerModelId, onSetRouterModel }: ModelCatalogBrowserProps) {
  // Store state
  const selectedModel = useModelCatalogStore((state) => state.selectedModel);
  const compatibleModels = useModelCatalogStore((state) => state.compatibleModels);
  const searchResults = useModelCatalogStore((state) => state.searchResults);
  const compatibleModelsLoading = useModelCatalogStore((state) => state.compatibleModelsLoading);
  const compatibleModelsError = useModelCatalogStore((state) => state.compatibleModelsError);
  const searchLoading = useModelCatalogStore((state) => state.searchLoading);
  const searchQuery = useModelCatalogStore((state) => state.searchQuery);
  const filters = useModelCatalogStore((state) => state.filters);
  const sortBy = useModelCatalogStore((state) => state.sortBy);

  // Actions
  const loadCompatibleModels = useModelCatalogStore((state) => state.loadCompatibleModels);
  const selectModel = useModelCatalogStore((state) => state.selectModel);
  const clearSelection = useModelCatalogStore((state) => state.clearSelection);
  const loadCacheStats = useModelCatalogStore((state) => state.loadCacheStats);

  // Load initial data
  useEffect(() => {
    loadCompatibleModels();
    loadCacheStats();
  }, [loadCompatibleModels, loadCacheStats]);

  // Determine which models to display
  const displayedModels = useMemo(() => {
    const sortModels = <T extends {
      ranking_score: number;
      popularity_downloads?: number | null;
      popularity_likes?: number | null;
      model: { id: string; size_gb: number; name: string };
    }>(
      models: T[]
    ) => {
      console.log('[ModelCatalogBrowser] Sorting', models.length, 'models by', sortBy);
      return [...models].sort((a, b) => {
        if (sortBy === 'name') {
          return a.model.name.localeCompare(b.model.name);
        }

        if (sortBy === 'size_asc') {
          return a.model.size_gb - b.model.size_gb;
        }

        if (sortBy === 'size_desc') {
          return b.model.size_gb - a.model.size_gb;
        }

        if (sortBy === 'likes') {
          const likesA = a.popularity_likes ?? 0;
          const likesB = b.popularity_likes ?? 0;
          if (likesB !== likesA) return likesB - likesA;
        }

        if (sortBy === 'popularity') {
          const downloadsA = a.popularity_downloads ?? 0;
          const downloadsB = b.popularity_downloads ?? 0;
          if (downloadsB !== downloadsA) return downloadsB - downloadsA;
        }

        if (sortBy === 'recommended') {
          // Blend ranking score with popularity so obscure models don't dominate
          const scoreA = a.ranking_score + Math.log10(Math.max(a.popularity_downloads ?? 1, 1)) * 5;
          const scoreB = b.ranking_score + Math.log10(Math.max(b.popularity_downloads ?? 1, 1)) * 5;
          if (scoreB !== scoreA) return scoreB - scoreA;
        }

        // Stable fallback: downloads then ranking
        const downloadsA = a.popularity_downloads ?? 0;
        const downloadsB = b.popularity_downloads ?? 0;
        if (downloadsB !== downloadsA) return downloadsB - downloadsA;
        return b.ranking_score - a.ranking_score;
      });
    };

    const applyFilters = <T extends {
      popularity_downloads?: number | null;
      model: { category: string; size_gb: number; capabilities: string[]; embedding_dimensions?: number | null };
    }>(models: T[]): T[] => {
      let filtered = models;

      if (filters.category) {
        filtered = filtered.filter((m) => m.model.category === filters.category);
      }

      if (filters.max_size_gb) {
        // Don't filter out unknown-size models (size_gb === 0)
        filtered = filtered.filter((m) => m.model.size_gb === 0 || m.model.size_gb <= filters.max_size_gb!);
      }

      if (filters.min_downloads != null) {
        filtered = filtered.filter((m) => (m.popularity_downloads ?? 0) >= filters.min_downloads!);
      }

      if (filters.required_capabilities.length > 0) {
        filtered = filtered.filter((m) =>
          filters.required_capabilities.every((cap) =>
            m.model.capabilities.some((modelCap) => modelCap.toLowerCase().includes(cap))
          )
        );
      }

      // Filter embedding models by dimension compatibility
      if (filters.embedding_dimensions != null) {
        filtered = filtered.filter((m) => {
          // Only filter embedding models; pass through LLM/OCR
          if (m.model.category !== 'Embedding') return true;
          // If model dimension is unknown, show it (user can decide)
          if (m.model.embedding_dimensions == null) return true;
          return m.model.embedding_dimensions === filters.embedding_dimensions;
        });
      }

      return filtered;
    };

    // If there's a search query and we have search results, show those
    if (searchQuery.trim() && searchResults.length > 0) {
      const mapped = searchResults.map((result) => ({
        model: result.model,
        compatibility: {
          compatibility_level: 'Good' as const,
          overall_score: result.relevance_score,
          ram_score: 0,
          gpu_score: 0,
          disk_score: 0,
          estimated_tokens_per_second: null,
          estimated_loading_time_seconds: 0,
          recommendations: [],
          blockers: [],
        },
        ranking_score: result.relevance_score,
        popularity_downloads: result.popularity_downloads ?? null,
        popularity_likes: result.popularity_likes ?? null,
      }));
      return sortModels(applyFilters(mapped));
    }

    return sortModels(applyFilters(compatibleModels));
  }, [compatibleModels, searchResults, searchQuery, filters, sortBy]);

  const isLoading = compatibleModelsLoading || searchLoading;
  const error = compatibleModelsError;

  // Detail view
  if (selectedModel) {
    return (
      <div className="space-y-6">
        <ModelDetailPanel
          model={selectedModel}
          onBack={clearSelection}
          routerModelId={routerModelId}
          onSetRouterModel={onSetRouterModel}
        />
      </div>
    );
  }

  // List view
  return (
    <div className="space-y-5">
      {/* System Capabilities */}
      <SystemCapabilitiesCard />

      {/* Search Bar */}
      <ModelSearchBar />

      {/* Two Column Layout */}
      <div className="grid grid-cols-1 xl:grid-cols-[300px_minmax(0,1fr)] gap-5 items-start">
        {/* Left Sidebar - Filters */}
        <div className="space-y-5 xl:sticky xl:top-0">
          <ModelFilterPanel />
          <CatalogManagementSection />
        </div>

        {/* Main Content - Model Grid */}
        <div className="min-w-0">
          {!isLoading && !error && displayedModels.length > 0 && (
            <div className="flex items-center justify-between mb-3">
              <span className="text-xs text-[var(--text-tertiary)]">
                {displayedModels.length} model{displayedModels.length !== 1 ? 's' : ''}
              </span>
            </div>
          )}
          <ModelListView
            models={displayedModels}
            loading={isLoading}
            error={error}
            selectedModelId={null}
            onModelSelect={selectModel}
          />
        </div>
      </div>
    </div>
  );
}
