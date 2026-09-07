/**
 * ModelSearchBar
 *
 * One field. Typing is debounced into the catalog search request.
 */

import { useCallback, useEffect, useState } from 'react';

import { X } from 'lucide-react';

import { cn } from '@/lib/utils';

import { useModelCatalog } from '../../../hooks/useModelCatalog';
import { settingsFieldClass } from '../../ui';

export function ModelSearchBar() {
  const { searchQuery, setSearchQuery, setFilters } = useModelCatalog({
    autoLoadCapabilities: false,
    autoLoadModels: false,
  });

  const [localQuery, setLocalQuery] = useState(searchQuery);

  const debouncedSearch = useCallback(
    (query: string) => {
      const handler = setTimeout(() => {
        setSearchQuery(query);
        setFilters({ query_text: query || null });
      }, 300);

      return () => clearTimeout(handler);
    },
    [setSearchQuery, setFilters]
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
      <input
        type="text"
        value={localQuery}
        onChange={(event) => setLocalQuery(event.target.value)}
        placeholder="Search models"
        aria-label="Search models"
        className={cn(settingsFieldClass, 'h-9', localQuery && 'pr-9')}
      />
      {localQuery ? (
        <button
          type="button"
          onClick={handleClear}
          aria-label="Clear search"
          className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted transition-colors duration-fast hover:text-text-primary"
        >
          <X className="h-4 w-4" />
        </button>
      ) : null}
    </div>
  );
}
