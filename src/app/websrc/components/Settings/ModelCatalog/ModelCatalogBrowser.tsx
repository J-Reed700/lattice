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
      model: { id: string };
    }>(
      models: T[]
    ) =>
      [...models].sort((a, b) => {
        if (sortBy === 'likes') {
          const likesA = a.popularity_likes ?? 0;
          const likesB = b.popularity_likes ?? 0;
          if (likesB !== likesA) {
            return likesB - likesA;
          }
        }

        if (sortBy === 'popularity') {
          const downloadsA = a.popularity_downloads ?? 0;
          const downloadsB = b.popularity_downloads ?? 0;
          if (downloadsB !== downloadsA) {
            return downloadsB - downloadsA;
          }
        }

        // Keep ordering stable and useful when popularity ties.
        const downloadsA = a.popularity_downloads ?? 0;
        const downloadsB = b.popularity_downloads ?? 0;
        if (downloadsB !== downloadsA) {
          return downloadsB - downloadsA;
        }

        return b.ranking_score - a.ranking_score;
      });

    // If there's a search query and we have search results, show those
    if (searchQuery.trim() && searchResults.length > 0) {
      // Convert search results to recommendations (they won't have compatibility scores)
      return sortModels(searchResults.map((result) => ({
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
      })));
    }

    // Otherwise show compatible models with filtering
    let filtered = compatibleModels;

    // Apply category filter
    if (filters.category) {
      filtered = filtered.filter((m) => m.model.category === filters.category);
    }

    // Apply size filter
    if (filters.max_size_gb) {
      filtered = filtered.filter((m) => m.model.size_gb <= filters.max_size_gb!);
    }

    // Apply capability filters
    if (filters.required_capabilities.length > 0) {
      filtered = filtered.filter((m) =>
        filters.required_capabilities.every((cap) =>
          m.model.capabilities.some((modelCap) => modelCap.toLowerCase().includes(cap))
        )
      );
    }

    return sortModels(filtered);
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
