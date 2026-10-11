import { useMemo } from 'react';

import { Search, SearchX } from 'lucide-react';

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
    if (recent.length === 0 || !onPickRecent) return (
      <div className="mt-8 flex items-start gap-4 rounded-xl border border-border-subtle bg-surface p-6">
        <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-accent-muted text-accent"><Search className="h-5 w-5" aria-hidden="true" /></span>
        <div>
          <h2 className="text-sm font-semibold text-text-primary">Search across your sources</h2>
          <p className="mt-1.5 max-w-[48ch] text-sm leading-relaxed text-text-secondary">Start with a topic, phrase, or question. Results point you back to the original document.</p>
          <p className="mt-3 text-xs leading-relaxed text-text-muted">Hybrid combines meaning and keywords. Semantic finds related ideas; Keyword matches words.</p>
        </div>
      </div>
    );
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

  return (
    <div className="flex items-start gap-3 rounded-lg border border-border-subtle bg-surface p-5">
      <SearchX className="mt-0.5 h-5 w-5 shrink-0 text-text-muted" aria-hidden="true" />
      <div className="min-w-0">
        <p className="wrap-break-word text-sm font-medium text-text-primary">No results for “{query}”.</p>
        <p className="mt-1 text-sm text-text-secondary">Try a broader term or a different search mode.</p>
      </div>
    </div>
  );
}
