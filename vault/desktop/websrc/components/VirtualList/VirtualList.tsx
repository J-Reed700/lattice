import { SearchResult } from '../SearchResult';

import type { SearchResult as SearchResultType } from '../../types';

interface VirtualizedSearchResultsProps {
  results: SearchResultType[];
  query?: string;
  onResultOpen?: (result: SearchResultType) => void;
}

export function VirtualizedSearchResults({
  results,
  query,
  onResultOpen,
}: VirtualizedSearchResultsProps) {
  return (
    <div className="flex flex-col gap-4 pb-4">
      {results.map((result) => (
        <SearchResult
          key={result.id}
          result={result}
          query={query}
          onOpen={onResultOpen}
        />
      ))}
    </div>
  );
}
