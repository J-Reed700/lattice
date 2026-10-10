import type { SearchResult as SearchResultType } from '@/types';

import { SearchResult } from '../SearchResult';


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
    <div className="border-t border-border-subtle">
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
