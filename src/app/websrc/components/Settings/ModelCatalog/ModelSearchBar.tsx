/**
 * ModelSearchBar
 *
 * Search input with debounce for model catalog
 */

import { useState, useEffect, useCallback } from 'react';

import { Search, X } from 'lucide-react';

import { useModelCatalogStore } from '../../../stores/modelCatalogStore';
import Input from '../../ui/input/Input';

export function ModelSearchBar() {
  const searchQuery = useModelCatalogStore((state) => state.searchQuery);
  const setSearchQuery = useModelCatalogStore((state) => state.setSearchQuery);
  const setFilters = useModelCatalogStore((state) => state.setFilters);
  const searchCatalog = useModelCatalogStore((state) => state.searchCatalog);

  const [localQuery, setLocalQuery] = useState(searchQuery);

  // Debounce search
  const debouncedSearch = useCallback(
    (query: string) => {
      const handler = setTimeout(() => {
        setSearchQuery(query);
        setFilters({ query_text: query || null });

        // If query is non-empty, trigger external search
        if (query.trim()) {
          searchCatalog({
            query: query.trim(),
            category: null,
            max_size_gb: null,
            limit: 20,
          });
        }
      }, 300);

      return () => clearTimeout(handler);
    },
    [setSearchQuery, setFilters, searchCatalog]
  );

  useEffect(() => {
    const cleanup = debouncedSearch(localQuery);
    return cleanup;
  }, [localQuery, debouncedSearch]);

  const handleClear = () => {
    setLocalQuery('');
    setSearchQuery('');
    setFilters({ query_text: null });
  };

  return (
    <div className="relative">
      <Input
        type="text"
        placeholder="Search models by name, capability, or description..."
        value={localQuery}
        onChange={(e) => setLocalQuery(e.target.value)}
        leftIcon={<Search className="w-4 h-4" />}
        className="w-full"
      />
      {localQuery && (
        <button
          onClick={handleClear}
          className="absolute right-3 top-1/2 -translate-y-1/2 text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors"
          aria-label="Clear search"
        >
          <X className="w-4 h-4" />
        </button>
      )}
    </div>
  );
}
