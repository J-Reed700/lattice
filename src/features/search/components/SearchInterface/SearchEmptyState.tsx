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
      <div className="mt-8 flex items-start gap-4 rounded-xl border border-border-subtle bg-surface p-6 shadow-sm">
        <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-accent-muted text-accent ring-1 ring-inset ring-accent/12"><Search className="h-5 w-5" aria-hidden="true" /></span>
        <div>
          <h2 className="font-sans text-sm font-semibold tracking-[-0.015em] text-text-primary">Search across your sources</h2>
          <p className="mt-1.5 max-w-[48ch] text-sm leading-relaxed text-text-secondary">Start with a topic, phrase, or question. Results point you back to the original document.</p>
          <p className="mt-3 text-xs leading-relaxed text-text-muted">Hybrid combines meaning and keywords. Semantic finds related ideas; Keyword matches words.</p>
        </div>
      </div>
    );
    return (
      <section className="mt-6">
        <SectionHeading>Recent</SectionHeading>
        <div className="overflow-hidden rounded-xl border border-border-subtle bg-surface shadow-sm">
          {recent.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onPickRecent(item.label)}
              className="row-hover flex w-full items-center border-b border-border-subtle px-3.5 py-2.5 text-left text-sm text-text-secondary transition-colors duration-fast last:border-b-0 hover:text-text-primary"
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
    <div className="flex items-start gap-3 rounded-xl border border-border-subtle bg-surface p-5 shadow-sm">
      <SearchX className="mt-0.5 h-5 w-5 shrink-0 text-text-muted" aria-hidden="true" />
      <div className="min-w-0">
        <p className="wrap-break-word font-sans text-sm font-semibold tracking-[-0.01em] text-text-primary">No results for “{query}”.</p>
        <p className="mt-1 text-sm text-text-secondary">Try a broader term or a different search mode.</p>
      </div>
    </div>
  );
}
