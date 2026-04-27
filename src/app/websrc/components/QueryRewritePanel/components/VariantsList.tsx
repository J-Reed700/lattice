import { Card } from '../../ui/card';
import { type QueryVariant } from '../types';
import { VariantCard } from './VariantCard';

interface VariantsListProps {
  variants: QueryVariant[];
  originalQuery: string;
  onVariantSelect: (query: string) => void;
  isGenerating: boolean;
  streamingResponse: string;
}

export function VariantsList({
  variants,
  originalQuery,
  onVariantSelect,
  isGenerating,
  streamingResponse,
}: VariantsListProps) {
  // Show variants if we have them
  if (variants.length > 0) {
    return (
      <div className="space-y-3">
        {variants.map((variant, index) => (
          <VariantCard
            key={index}
            variant={variant}
            index={index + 1}
            originalQuery={originalQuery}
            onSelect={() => onVariantSelect(variant.query)}
            isGenerating={isGenerating}
          />
        ))}
      </div>
    );
  }

  // Show empty state if we have a response but no parsed variants
  if (!isGenerating && streamingResponse) {
    return (
      <Card className="border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))]/20">
        <div className="p-4">
          <p className="text-sm text-[hsl(var(--warning-fg))]">
            Could not generate alternative queries. The response may not be in the expected
            format.
          </p>
        </div>
      </Card>
    );
  }

  return null;
}
