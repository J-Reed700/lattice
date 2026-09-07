import { useMemo } from 'react';

import { SectionHeading } from '@/components/ui';

interface SearchEmptyStateProps {
  hasSearched: boolean;
  query: string;
  isSearching: boolean;
  /** Called with a recent query when the user picks one. */
  onPickRecent?: (query: string) => void;
}

interface RecentItem {
  id: string;
  label: string;
}

/** Recent searches are kept by the command palette under this key. */
const RECENT_STORAGE_KEY = 'lattice-command-palette';

function readRecentSearches(): RecentItem[] {
  try {
    const raw = localStorage.getItem(RECENT_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as { recentSearches?: unknown };
    if (!Array.isArray(parsed.recentSearches)) return [];
    return parsed.recentSearches
      .filter((item): item is RecentItem => Boolean(item && typeof item === 'object' && typeof (item as RecentItem).label === 'string'))
      .slice(0, 8);
  } catch {
    return [];
  }
}

export function SearchEmptyState({ hasSearched, query, isSearching, onPickRecent }: SearchEmptyStateProps) {
  const recent = useMemo(() => (hasSearched ? [] : readRecentSearches()), [hasSearched]);

  if (!hasSearched) {
    if (recent.length === 0 || !onPickRecent) return null;
    return (
      <section className="mt-6">
        <SectionHeading>Recent</SectionHeading>
        <div className="border-t border-border-subtle">
          {recent.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onPickRecent(item.label)}
              className="flex w-full items-center border-b border-border-subtle px-2 py-2.5 text-left text-sm text-text-secondary transition-colors duration-fast hover:bg-surface hover:text-text-primary"
            >
              {item.label}
            </button>
          ))}
        </div>
      </section>
    );
  }

  if (isSearching) return null;

  return <p className="pt-6 text-sm text-text-muted">No results for “{query}”.</p>;
}
