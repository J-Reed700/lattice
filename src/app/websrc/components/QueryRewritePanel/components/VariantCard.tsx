import { Card } from '../../ui/card';
import { type QueryVariant } from '../types';

interface VariantCardProps {
  variant: QueryVariant;
  index: number;
  originalQuery: string;
  onSelect: () => void;
  isGenerating: boolean;
}

export function VariantCard({
  variant,
  index,
  originalQuery,
  onSelect,
  isGenerating,
}: VariantCardProps) {
  // Highlight differences between original and variant
  const highlightDifferences = (original: string, variant: string): React.ReactNode => {
    const originalWords = original.toLowerCase().split(/\s+/);
    const variantWords = variant.split(/\s+/);

    return variantWords.map((word, idx) => {
      const isNew = !originalWords.some((ow) => ow === word.toLowerCase());
      return (
        <span
          key={idx}
          className={isNew ? 'bg-[var(--accent-light)]/40 px-1 rounded' : ''}
        >
          {word}
          {idx < variantWords.length - 1 ? ' ' : ''}
        </span>
      );
    });
  };

  return (
    <Card
      className="cursor-pointer hover:shadow-md hover:border-[var(--accent-light)] transition-all"
      onClick={onSelect}
    >
      <div className="flex items-start gap-3 p-4">
        <div className="flex-shrink-0">
          <div className="w-8 h-8 bg-[var(--accent-light)]/40 rounded-full flex items-center justify-center">
            <span className="text-sm font-bold text-[var(--accent-primary)]">{index}</span>
          </div>
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-2">
            <svg
              className="w-4 h-4 text-[var(--accent-primary)]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M13 10V3L4 14h7v7l9-11h-7z"
              />
            </svg>
            <span className="text-xs font-semibold text-[var(--accent-primary)] uppercase tracking-wide">
              Variant {index}
            </span>
          </div>
          <p className="text-base font-medium text-[var(--text-primary)] mb-2">
            {highlightDifferences(originalQuery, variant.query)}
          </p>
          <div className="flex items-start gap-2">
            <svg
              className="w-4 h-4 text-[var(--text-tertiary)] flex-shrink-0 mt-0.5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            <p className="text-sm text-[var(--text-secondary)] flex-1">{variant.reasoning}</p>
          </div>
          <div className="mt-2 text-xs text-[var(--text-secondary)]">
            Press {index} to search with this query
          </div>
        </div>
        {isGenerating && (
          <div className="flex-shrink-0">
            <svg
              className="animate-pulse h-5 w-5 text-[var(--accent-primary)]"
              fill="currentColor"
              viewBox="0 0 24 24"
            >
              <circle cx="12" cy="12" r="10" opacity="0.25" />
              <path d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
            </svg>
          </div>
        )}
      </div>
    </Card>
  );
}
