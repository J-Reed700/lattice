import type { SearchResult as SearchResultType } from '@/types';

import { SearchResult } from '../SearchResult';

interface VirtualizedSearchResultsProps {
  results: SearchResultType[];
  query?: string;
  selectedId?: string | null;
  showQuickPreviewAction?: boolean;
  onResultSelect?: (result: SearchResultType) => void;
  onResultOpen?: (result: SearchResultType) => void;
}

export function VirtualizedSearchResults({
  results,
  query,
  selectedId,
  showQuickPreviewAction = false,
  onResultSelect,
  onResultOpen,
}: VirtualizedSearchResultsProps) {
  return (
    <div className="border-t border-border-subtle">
      {results.map((result) => (
        <SearchResult
          key={result.id}
          result={result}
          query={query}
          isSelected={selectedId === result.id}
          showQuickPreviewAction={showQuickPreviewAction}
          onSelect={onResultSelect}
          onOpen={onResultOpen}
        />
      ))}
    </div>
  );
}
