/**
 * ModelListView
 *
 * The catalog results: hairline rows, one line of muted text for empty,
 * loading, and error. Owns the download state for the rows so the list
 * subscribes once rather than once per row.
 */

import { useCallback, useMemo, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { CATALOG_TEXT_BUTTON_CLASS } from './catalogUtils';
import { ModelCatalogResults } from './ModelCatalogResults';
import { ModelRow } from './ModelRow';
import { startModelDownload } from './startModelDownload';
import { useHuggingFaceTokenStatusQuery } from '../../../hooks/queries/useHuggingFaceTokenQuery';
import { useDownloadedModels } from '../../../hooks/useDownloadedModels';
import { useDownloadState } from '../../../hooks/useDownloadState';
import { useModelCatalog } from '../../../hooks/useModelCatalog';
import { useToastStore } from '../../../stores/toastStore';
import { Skeleton } from '../../ui/Skeleton/Skeleton';

import type { CatalogResultsNavigation } from './ModelCatalogResults';
import type { ModelRecommendation } from '../../../types/modelCatalog';

interface ModelListViewProps extends CatalogResultsNavigation {
  models: ModelRecommendation[];
  loading: boolean;
  error?: string | null;
  onModelSelect: (model: ModelRecommendation) => void;
  /** Shown next to the empty state so a filtered-to-nothing list has a way out. */
  filtersActive?: boolean;
  onResetFilters?: () => void;
  /** Retry the catalog fetch after a failure. */
  onRetry?: () => void;
  /** Sends the user to the Hugging Face token field on this page. */
  onAddToken?: () => void;
}

export function ModelListView({
  models,
  loading,
  error,
  onModelSelect,
  filtersActive = false,
  onResetFilters,
  onRetry,
  onAddToken,
  overview,
  page,
  onPageChange,
  onBrowseCategory,
}: ModelListViewProps) {
  const { downloadedModels } = useDownloadedModels();
  const { getActiveDownloadForModel } = useDownloadState();
  // Read the machine once for the whole list rather than once per row.
  const { systemCapabilities } = useModelCatalog({
    autoLoadCapabilities: true,
    autoLoadModels: false,
  });
  const { data: tokenStatus } = useHuggingFaceTokenStatusQuery();
  const addToast = useToastStore((state) => state.addToast);
  const queryClient = useQueryClient();
  const [startingIds, setStartingIds] = useState<ReadonlySet<string>>(new Set());

  const downloadedIds = useMemo(() => {
    const ids = new Set<string>();
    for (const model of downloadedModels) {
      ids.add(model.id);
      if (model.model_id) ids.add(model.model_id);
    }
    return ids;
  }, [downloadedModels]);

  const handleDownload = useCallback(
    async (model: ModelRecommendation) => {
      const modelId = model.model.id;
      if (startingIds.has(modelId)) return;
      setStartingIds((previous) => new Set(previous).add(modelId));
      try {
        await startModelDownload({ metadata: model.model, addToast, queryClient });
      } finally {
        setStartingIds((previous) => {
          const next = new Set(previous);
          next.delete(modelId);
          return next;
        });
      }
    },
    [addToast, queryClient, startingIds],
  );

  if (loading) {
    return (
      <div className="border-t border-border-subtle">
        {[1, 2, 3, 4, 5].map((index) => (
          <div key={index} className="border-b border-border-subtle py-3">
            <Skeleton variant="text" width="34%" height="1rem" />
            <div className="mt-1.5">
              <Skeleton variant="text" width="22%" height="0.75rem" />
            </div>
            <div className="mt-1.5">
              <Skeleton variant="text" width="46%" height="0.75rem" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center gap-2 border-t border-border-subtle py-3">
        <p className="text-sm text-danger-fg">Couldn&apos;t load the catalog. {error}</p>
        {onRetry ? (
          <button type="button" onClick={onRetry} className={CATALOG_TEXT_BUTTON_CLASS}>
            Retry
          </button>
        ) : null}
      </div>
    );
  }

  if (models.length === 0) {
    return (
      <div className="flex items-center gap-2 border-t border-border-subtle py-3">
        <p className="text-sm text-text-muted">No models match.</p>
        {filtersActive && onResetFilters ? (
          <button type="button" onClick={onResetFilters} className={CATALOG_TEXT_BUTTON_CLASS}>
            Reset filters
          </button>
        ) : null}
      </div>
    );
  }

  return (
    <ModelCatalogResults
      models={models}
      overview={overview}
      page={page}
      onPageChange={onPageChange}
      onBrowseCategory={onBrowseCategory}
      renderModel={(model) => (
        <ModelRow
          key={model.model.id}
          model={model}
          compact={overview}
          onSelect={() => onModelSelect(model)}
          isDownloaded={downloadedIds.has(model.model.id)}
          activeDownload={getActiveDownloadForModel(model.model.id)}
          isStarting={startingIds.has(model.model.id)}
          onDownload={() => void handleDownload(model)}
          capabilities={systemCapabilities}
          hasHfToken={tokenStatus?.isSet ?? false}
          onAddToken={onAddToken}
        />
      )}
    />
  );
}
