interface SearchEmptyStateProps {
  hasSearched: boolean;
  searchMode: string;
  isSearching: boolean;
}

export function SearchEmptyState({ hasSearched, searchMode, isSearching }: SearchEmptyStateProps) {
  if (!hasSearched) {
    return (
      <div className="text-center text-[hsl(var(--text-secondary))] mt-20">
        <svg className="mx-auto w-16 h-16 text-[hsl(var(--text-tertiary))] mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <p className="text-lg">Start typing to search your files</p>
        <p className="text-sm mt-2">Using {searchMode} search mode</p>
      </div>
    );
  }

  if (!isSearching) {
    return (
      <div className="text-center text-[hsl(var(--text-secondary))] mt-20">
        <svg className="mx-auto w-16 h-16 text-[hsl(var(--text-tertiary))] mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <p className="text-lg">No results found</p>
        <p className="text-sm mt-2">Try a different search term or mode</p>
      </div>
    );
  }

  return null;
}
