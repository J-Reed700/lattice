import { useId } from 'react';

import { RecommendationCardBody } from '@/features/model/components/RecommendationCardBody';
import type { RecommendationDto } from '@/lib/bindings';
import type { ModelRecommendation } from '@/types/modelCatalog';
import { VISION_CAPABILITY } from '@/utils/visionGeometry';

interface RecommendedSectionProps {
  recommendations: RecommendationDto[];
  models: ModelRecommendation[];
  onSelect: (model: ModelRecommendation) => void;
}

/** The backend's ranked list for this computer, above the rest of the catalog. */
export function RecommendedSection({ recommendations, models, onSelect }: RecommendedSectionProps) {
  const headingId = useId();
  if (recommendations.length === 0) return null;
  return (
    <section aria-labelledby={headingId} className="space-y-2">
      <h3 id={headingId} className="text-base font-medium text-text-primary">Recommended for this computer</h3>
      <ul className="space-y-2">
        {recommendations.map((entry) => {
          const row = models.find((candidate) => candidate.model.id === entry.modelId);
          const body = (
            <RecommendationCardBody
              recommendation={entry}
              seesImages={Boolean(row?.model.capabilities.includes(VISION_CAPABILITY))}
            />
          );
          return (
            <li key={entry.modelId}>
              {row ? (
                <button
                  type="button"
                  onClick={() => onSelect(row)}
                  className="flex w-full rounded-lg border border-border-subtle px-3.5 py-3 text-left hover:bg-surface-hover focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                >
                  {body}
                </button>
              ) : (
                <div className="flex rounded-lg border border-border-subtle px-3.5 py-3">{body}</div>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
