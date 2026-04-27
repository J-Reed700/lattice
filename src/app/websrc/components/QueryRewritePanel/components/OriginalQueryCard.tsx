import { Card } from '../../ui/card';

interface OriginalQueryCardProps {
  query: string;
  onSelect: () => void;
}

export function OriginalQueryCard({ query, onSelect }: OriginalQueryCardProps) {
  return (
    <Card
      className="cursor-pointer hover:shadow-md transition-shadow"
      onClick={onSelect}
    >
      <div className="flex items-start gap-3 p-4">
        <div className="flex-shrink-0">
          <div className="w-8 h-8 bg-[hsl(var(--surface-raised))] rounded-full flex items-center justify-center">
            <span className="text-sm font-bold text-[hsl(var(--text-secondary))]">0</span>
          </div>
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <svg
              className="w-4 h-4 text-[hsl(var(--text-tertiary))]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
            <span className="text-xs font-semibold text-[hsl(var(--text-secondary))] uppercase tracking-wide">
              Original Query
            </span>
          </div>
          <p className="text-base font-medium text-[hsl(var(--text-primary))] mb-1">{query}</p>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Press 0 to search with original query
          </p>
        </div>
      </div>
    </Card>
  );
}
