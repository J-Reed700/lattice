import { useState, useCallback, useEffect, useRef, useMemo } from 'react';

import { debounce } from '../../lib/debounce';
import { Input } from '../ui/input';

/**
 * SearchBar
 *
 * Purpose: Primary search interface for document discovery
 *
 * Features:
 * - Real-time search with debouncing (300ms)
 * - Three search modes: semantic, keyword, hybrid
 * - Visual loading indicator
 * - Keyboard shortcuts (Cmd+K for focus, Cmd+F for search mode)
 * - Auto-focus on mount
 * - Search suggestions (future enhancement)
 *
 * States: idle, searching, error
 * Accessibility: WCAG AA, keyboard navigation, screen reader announcements
 */

interface SearchBarProps {
  onSearch: (query: string, mode: 'semantic' | 'keyword' | 'hybrid') => void;
  isSearching?: boolean;
  placeholder?: string;
  autoFocus?: boolean;
}

export function SearchBar({
  onSearch,
  isSearching = false,
  placeholder = 'Search your documents...',
  autoFocus = true,
}: SearchBarProps) {
  const [query, setQuery] = useState('');
  const [searchMode, setSearchMode] = useState<'semantic' | 'keyword' | 'hybrid'>('hybrid');
  const inputRef = useRef<HTMLInputElement>(null);

  // Use ref to hold latest props to avoid stale closures
  const latestPropsRef = useRef({ onSearch, searchMode });
  latestPropsRef.current = { onSearch, searchMode };

  // Create stable debounced function that always uses latest props
  const debouncedSearch = useMemo(
    () => debounce((searchQuery: string, mode: 'semantic' | 'keyword' | 'hybrid') => {
      latestPropsRef.current.onSearch(searchQuery.trim(), mode);
    }, 300),
    []
  );

  // Cleanup on unmount
  useEffect(() => () => {
      debouncedSearch.cancel();
    }, [debouncedSearch]);

  // Execute search
  const executeSearch = useCallback(
    (searchQuery: string) => {
      if (!searchQuery.trim()) {
        // Call immediately for empty searches
        onSearch('', searchMode);
        return;
      }
      debouncedSearch(searchQuery, searchMode);
    },
    [debouncedSearch, onSearch, searchMode]
  );

  // Handle query changes
  const handleQueryChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const newQuery = e.target.value;
      setQuery(newQuery);
      executeSearch(newQuery);
    },
    [executeSearch]
  );

  // Handle search mode changes
  const handleModeChange = useCallback(
    (mode: 'semantic' | 'keyword' | 'hybrid') => {
      setSearchMode(mode);
      if (query.trim()) {
        onSearch(query.trim(), mode);
      }
    },
    [query, onSearch]
  );


  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd/Ctrl + K to focus search
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        inputRef.current?.focus();
      }

      // Cmd/Ctrl + F to cycle search modes
      if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
        e.preventDefault();
        setSearchMode((prev) => {
          const modes: Array<'semantic' | 'keyword' | 'hybrid'> = ['semantic', 'keyword', 'hybrid'];
          const currentIndex = modes.indexOf(prev);
          const nextMode = modes[(currentIndex + 1) % modes.length];
          if (query.trim()) {
            onSearch(query.trim(), nextMode);
          }
          return nextMode;
        });
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [query, onSearch]);

  const searchIcon = (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
      />
    </svg>
  );

  return (
    <div className="w-full space-y-4">
      {/* Search Input */}
      <Input
        ref={inputRef}
        type="text"
        value={query}
        onChange={handleQueryChange}
        placeholder={placeholder}
        leftIcon={searchIcon}
        isLoading={isSearching}
        autoFocus={autoFocus}
        aria-label="Search documents"
        aria-describedby="search-mode-selector"
      />

      {/* Search Mode Selector */}
      <div
        id="search-mode-selector"
        className="flex items-center gap-2"
        role="group"
        aria-label="Search mode selection"
      >
        <span className="text-sm text-[var(--text-secondary)] font-medium">
          Search mode:
        </span>

        <div className="flex gap-2">
          <ModeButton
            active={searchMode === 'semantic'}
            onClick={() => handleModeChange('semantic')}
            label="Semantic"
            description="Meaning-based search using AI embeddings"
          />
          <ModeButton
            active={searchMode === 'keyword'}
            onClick={() => handleModeChange('keyword')}
            label="Keyword"
            description="Fast exact keyword matching"
          />
          <ModeButton
            active={searchMode === 'hybrid'}
            onClick={() => handleModeChange('hybrid')}
            label="Hybrid"
            description="Combines semantic and keyword search"
          />
        </div>
      </div>

      {/* Keyboard Shortcuts Hint */}
      <div className="flex items-center gap-4 text-xs text-[var(--text-secondary)]">
        <kbd className="px-2 py-1 bg-[var(--bg-primary)] border border-[var(--border-color)] rounded">
          Cmd+K
        </kbd>
        <span>Focus search</span>
        <kbd className="px-2 py-1 bg-[var(--bg-primary)] border border-[var(--border-color)] rounded">
          Cmd+F
        </kbd>
        <span>Cycle modes</span>
      </div>
    </div>
  );
}

// Search mode button component
interface ModeButtonProps {
  active: boolean;
  onClick: () => void;
  label: string;
  description: string;
}

function ModeButton({ active, onClick, label, description }: ModeButtonProps) {
  return (
    <button
      onClick={onClick}
      title={description}
      aria-pressed={active}
      className={`
        px-3 py-1.5 text-sm font-medium rounded-lg
        transition-all duration-150
        focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)] focus-visible:ring-offset-2
        ${
          active
            ? 'bg-[var(--accent-primary)] text-white shadow-sm'
            : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]'
        }
      `}
    >
      {label}
    </button>
  );
}
