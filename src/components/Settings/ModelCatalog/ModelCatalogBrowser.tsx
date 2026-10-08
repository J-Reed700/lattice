/**
 * ModelCatalogBrowser
 *
 * The catalog: this machine's specs, a search field, one row of filters, the
 * results, and the cache. Sorting and filtering happen here so the list and
 * the filter row stay presentational.
 */

import { useCallback, useMemo, useState } from 'react';

import { ChevronDown, Monitor } from 'lucide-react';

import { CatalogManagementSection } from './CatalogManagementSection';
import { CATALOG_TEXT_BUTTON_CLASS, computeModelFit, hasActiveFilters } from './catalogUtils';
import { ModelDetailPanel } from './ModelDetailPanel';
import { ModelFilterPanel } from './ModelFilterPanel';
import { ModelListView } from './ModelListView';
import { ModelSearchBar } from './ModelSearchBar';
import { modelQuantization } from './quantization';
import { SystemCapabilitiesCard } from './SystemCapabilitiesCard';
import { useModelCatalog } from '../../../hooks/useModelCatalog';
import { useModelCatalogStore } from '../../../stores/modelCatalogStore';
import './modelCatalog.css';

interface ModelCatalogBrowserProps {
  routerModelId?: string;
  onSetRouterModel?: (_modelId: string) => Promise<void>;
}

export function ModelCatalogBrowser({ routerModelId, onSetRouterModel }: ModelCatalogBrowserProps) {
  const {
    selectedModel,
    systemCapabilities,
    compatibleModels,
    searchResults,
    compatibleModelsLoading,
    compatibleModelsError,
    searchLoading,
    searchError,
    reloadSearch,
    searchQuery,
    filters,
    sortBy,
    setFilters,
    selectModel,
    clearSelection,
    loadCompatibleModels,
  } = useModelCatalog({ loadCacheStats: true });
  const { quantizationFilter, fitFilter, resetFilters } = useModelCatalogStore();
  const [browseAll, setBrowseAll] = useState(false);
  const [pagination, setPagination] = useState({ key: '', page: 1 });
  const resultsKey = JSON.stringify([searchQuery, filters, sortBy, browseAll, quantizationFilter, fitFilter]);
  const page = pagination.key === resultsKey ? pagination.page : 1;
  const isSearching = searchQuery.trim().length > 0;
  const filtersActive = hasActiveFilters(filters) || Boolean(quantizationFilter) || fitFilter !== 'all';
  const overview = !browseAll && !isSearching && !filtersActive;

  // The catalog and the token field share this page, so "Add token" is a scroll,
  // not a navigation.
  const focusTokenField = useCallback(() => {
    const field = document.getElementById('hf-token');
    field?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    field?.focus();
  }, []);

  // Determine which models to display
  const displayedModels = useMemo(() => {
    const sortModels = <T extends {
      ranking_score: number;
      popularity_downloads?: number | null;
      popularity_likes?: number | null;
      model: { id: string; size_gb: number; name: string };
    }>(
      models: T[]
    ) => [...models].sort((a, b) => {
        if (sortBy === 'name') {
          return a.model.name.localeCompare(b.model.name);
        }

        if (sortBy === 'size_asc') {
          return (a.model.size_gb || Infinity) - (b.model.size_gb || Infinity);
        }

        if (sortBy === 'size_desc') {
          return (b.model.size_gb || -Infinity) - (a.model.size_gb || -Infinity);
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

    const applyFilters = <T extends {
      popularity_downloads?: number | null;
      model: {
        category: string;
        size_gb: number;
        capabilities: string[];
        performance_tier: string;
        embedding_dimensions?: number | null;
      };
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
        // The speed filter writes a performance tier here, so a model matches
        // on either its declared capabilities or its tier.
        filtered = filtered.filter((m) =>
          filters.required_capabilities.every(
            (cap) =>
              m.model.capabilities.some((modelCap) => modelCap.toLowerCase().includes(cap)) ||
              m.model.performance_tier.toLowerCase() === cap
          )
        );
      }

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
    if (searchQuery.trim()) {
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

  const quantizations = useMemo(() => [...new Set((isSearching ? searchResults : compatibleModels)
    .map(({ model }) => modelQuantization(model)).filter((value): value is string => Boolean(value)))], [isSearching, searchResults, compatibleModels]);
  const filteredModels = displayedModels.filter(({ model }) => {
    if (quantizationFilter && modelQuantization(model) !== quantizationFilter) return false;
    if (fitFilter === 'all') return true;
    const verdict = computeModelFit(model, systemCapabilities)?.verdict;
    return verdict === 'fits' || (fitFilter === 'fits-or-tight' && verdict === 'tight');
  });

  const isLoading = isSearching ? searchLoading : compatibleModelsLoading;
  const error = isSearching ? searchError : compatibleModelsError;

  if (selectedModel) {
    return (
      <ModelDetailPanel
        key={selectedModel.model.id}
        model={selectedModel}
        onBack={clearSelection}
        routerModelId={routerModelId}
        onSetRouterModel={onSetRouterModel}
        onAddToken={focusTokenField}
      />
    );
  }

  return (
    <div className="space-y-5">
      <details className="catalog-computer group rounded-lg border border-border-subtle bg-surface">
        <summary className="flex cursor-pointer list-none flex-wrap items-center gap-x-3 gap-y-1 px-4 py-3 text-xs text-text-secondary">
          <Monitor className="h-4 w-4 text-accent" aria-hidden="true" />
          <span className="font-medium text-text-primary">This computer</span>
          {systemCapabilities ? <span className="tabular-nums">{Math.round(systemCapabilities.total_ram_gb)} GB memory · {systemCapabilities.gpu_acceleration}</span> : <span>Hardware and available memory</span>}
          <ChevronDown className="ml-auto h-3.5 w-3.5 transition-transform group-open:rotate-180" aria-hidden="true" />
        </summary>
        <div className="px-4 pb-2"><SystemCapabilitiesCard /></div>
      </details>
      <div className="space-y-4 rounded-xl border border-border-default bg-surface p-4 shadow-sm">
        <ModelSearchBar />
        <ModelFilterPanel quantizations={quantizations} />
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 className="text-base font-medium text-text-primary">
            {overview ? 'Explore by purpose' : isSearching ? 'Search results' : 'Browse models'}
          </h3>
          {overview ? <p className="mt-1 text-xs text-text-muted">Choose a purpose, then compare models and versions.</p> : null}
        </div>
        {!isSearching && !filtersActive ? (
          <button type="button" onClick={() => setBrowseAll((previous) => !previous)} className={CATALOG_TEXT_BUTTON_CLASS}>
            {browseAll ? 'Explore categories' : 'Browse all models'}
          </button>
        ) : null}
      </div>
      <ModelListView
        models={filteredModels}
        loading={isLoading}
        error={error}
        onModelSelect={selectModel}
        overview={overview}
        page={page}
        onPageChange={(nextPage) => setPagination({ key: resultsKey, page: nextPage })}
        onBrowseCategory={(category) => setFilters({ category })}
        filtersActive={filtersActive}
        onResetFilters={resetFilters}
        // The hook surfaces the failure in `compatibleModelsError`; catching
        // keeps a second consecutive failure from becoming an unhandled
        // rejection.
        onRetry={() => {
          (isSearching ? reloadSearch() : loadCompatibleModels()).catch(() => undefined);
        }}
        onAddToken={focusTokenField}
      />
      <details className="rounded-lg border border-border-subtle bg-surface px-4 py-3">
        <summary className="cursor-pointer text-xs font-medium text-text-secondary">Catalog maintenance</summary>
        <div className="mt-3"><CatalogManagementSection /></div>
      </details>
    </div>
  );
}
