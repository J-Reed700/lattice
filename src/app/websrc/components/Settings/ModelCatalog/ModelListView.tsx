/**
 * ModelListView
 *
 * Grid display of model cards with loading states
 */

import { AlertCircle, Inbox } from 'lucide-react';

import { ModelCard } from './ModelCard';
import { SkeletonCard } from '../../ui/Skeleton/Skeleton';

import type { ModelRecommendation } from '../../../types/modelCatalog';


interface ModelListViewProps {
  models: ModelRecommendation[];
  loading: boolean;
  error?: string | null;
  selectedModelId?: string | null;
  onModelSelect: (model: ModelRecommendation) => void;
}

export function ModelListView({
  models,
  loading,
  error,
  selectedModelId,
  onModelSelect,
}: ModelListViewProps) {
  const gridClassName = 'grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-4 md:gap-5';

  // Loading state
  if (loading) {
    return (
      <div className={gridClassName}>
        {[1, 2, 3, 4, 5, 6].map((i) => (
          <SkeletonCard key={i} showActions={false} />
        ))}
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-center">
        <AlertCircle className="w-12 h-12 text-[var(--error)] mb-4" />
        <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
          Failed to Load Models
        </h3>
        <p className="text-sm text-[var(--text-secondary)] max-w-md">
          {error}
        </p>
      </div>
    );
  }

  // Empty state
  if (!models || models.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-center">
        <Inbox className="w-12 h-12 text-[var(--text-tertiary)] mb-4" />
        <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
          No Models Found
        </h3>
        <p className="text-sm text-[var(--text-secondary)] max-w-md">
          Try adjusting your filters or search query to find more models.
        </p>
      </div>
    );
  }

  // Grid of model cards
  return (
    <div className={gridClassName}>
      {models.map((model) => (
        <ModelCard
          key={model.model.id}
          model={model}
          onClick={() => onModelSelect(model)}
          isSelected={selectedModelId === model.model.id}
        />
      ))}
    </div>
  );
}
