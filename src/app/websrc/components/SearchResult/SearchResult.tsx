import { memo, useCallback, useMemo } from 'react';

import type { SearchResult as SearchResultType } from '@/types';

interface SearchResultProps {
  result: SearchResultType;
  query?: string;
  onOpen?: (result: SearchResultType) => void;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function termEntropy(term: string): number {
  if (!term) return 0;
  const counts = new Map<string, number>();
  for (const ch of term) {
    counts.set(ch, (counts.get(ch) || 0) + 1);
  }
  let entropy = 0;
  for (const count of counts.values()) {
    const p = count / term.length;
    entropy -= p * Math.log2(p);
  }
  return entropy;
}

function termSalience(term: string): number {
  const entropy = termEntropy(term);
  const lengthFactor = Math.log(term.length + 1);
  return entropy * (0.65 + 0.35 * lengthFactor);
}

function selectInformativeTerms(terms: string[], maxTerms: number): string[] {
  const normalized = Array.from(
    new Set(
      terms
        .map((term) => term.trim().toLowerCase())
        .filter((term) => term.length >= 3 && /[a-z]/i.test(term))
    )
  );
  if (normalized.length === 0) return [];

  const ranked = normalized
    .map((term) => ({ term, score: termSalience(term) }))
    .sort((a, b) => b.score - a.score || a.term.localeCompare(b.term));

  const bestScore = ranked[0]?.score ?? 0;
  if (bestScore <= Number.EPSILON) {
    return ranked.slice(0, maxTerms).map((entry) => entry.term);
  }

  const selected = ranked
    .filter((entry) => entry.score >= bestScore * 0.45)
    .slice(0, maxTerms)
    .map((entry) => entry.term);

  if (selected.length > 0) return selected;
  return ranked.slice(0, maxTerms).map((entry) => entry.term);
}

function SearchResultComponent({ result, query, onOpen }: SearchResultProps) {
  const handleClick = useCallback(() => {
    onOpen?.(result);
  }, [onOpen, result]);

  const formatScore = useCallback((score: number) => (score * 100).toFixed(1), []);

  const scoreColor = useMemo(() => {
    if (result.score >= 0.8) return 'text-[var(--success)]';
    if (result.score >= 0.6) return 'text-[var(--warning)]';
    return 'text-[var(--text-secondary)]';
  }, [result.score]);

  const highlightedExcerpts = useMemo(() => {
    const baseExcerpts = (result.highlights && result.highlights.length > 0)
      ? result.highlights
      : (result.content ? [result.content] : []);

    const trimmed = baseExcerpts
      .map((excerpt) => excerpt.trim())
      .filter((excerpt) => excerpt.length > 0);

    return trimmed.slice(0, 3);
  }, [result.content, result.highlights]);

  const queryTerms = useMemo(() => {
    if (!query) {
      return [];
    }

    const terms = query
      .split(/\s+/)
      .map((term) => term.trim())
      .map((term) => term.toLowerCase());
    return selectInformativeTerms(terms, 10);
  }, [query]);

  const renderHighlightedText = useCallback((text: string) => {
    if (queryTerms.length === 0) {
      return text;
    }

    const escapedTerms = queryTerms.map(escapeRegExp);
    if (escapedTerms.length === 0) {
      return text;
    }

    const splitRegex = new RegExp(`(${escapedTerms.join('|')})`, 'gi');
    const matchRegex = new RegExp(`^(${escapedTerms.join('|')})$`, 'i');
    const parts = text.split(splitRegex);

    return parts.map((part, idx) => {
      if (matchRegex.test(part)) {
        return (
          <mark key={`${part}-${idx}`} className="bg-[var(--warning)]/30 text-[var(--text-primary)] rounded px-0.5">
            {part}
          </mark>
        );
      }
      return <span key={`${part}-${idx}`}>{part}</span>;
    });
  }, [queryTerms]);

  const displayTitle = (result.title && result.title !== result.id)
    ? result.title
    : (result.metadata.filename as string || result.id);
  const displayPath = result.path || (result.metadata.path as string | undefined);

  return (
    <button
      type="button"
      className="w-full text-left bg-[var(--surface-elevated)] p-5 rounded-lg shadow hover:shadow-md transition-shadow cursor-pointer border border-[var(--border-color)]"
      onClick={handleClick}
    >
      <div className="flex items-start justify-between mb-3">
        <div className="flex-1">
          <h3 className="font-semibold text-lg text-[var(--text-primary)] mb-1">
            {displayTitle}
          </h3>
          {displayPath && (
            <p className="text-xs text-[var(--text-secondary)] truncate">
              {displayPath}
            </p>
          )}
        </div>
        <div className="ml-4 flex flex-col items-end">
          <div className={`text-sm font-medium ${scoreColor}`}>
            {formatScore(result.score)}%
          </div>
          {result.metadata.file_type && (
            <span className="text-xs text-[var(--text-secondary)] mt-1 uppercase">
              {result.metadata.file_type as string}
            </span>
          )}
        </div>
      </div>

      {highlightedExcerpts.length > 0 && (
        <div className="mb-3">
          {highlightedExcerpts.map((excerpt, index) => (
            <p className="text-sm text-[var(--text-secondary)] line-clamp-3" key={`${result.id}-excerpt-${index}`}>
              {renderHighlightedText(excerpt)}
            </p>
          ))}
        </div>
      )}

      <div className="flex flex-wrap gap-2 items-center text-xs text-[var(--text-secondary)]">
        {result.vectorScore != null && (
          <div className="flex items-center gap-1 bg-[var(--accent-light)]/30 text-[var(--accent-primary)] px-2 py-1 rounded">
            <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
              <path d="M9 2a1 1 0 000 2h2a1 1 0 100-2H9z" />
              <path
                fillRule="evenodd"
                d="M4 5a2 2 0 012-2 3 3 0 003 3h2a3 3 0 003-3 2 2 0 012 2v11a2 2 0 01-2 2H6a2 2 0 01-2-2V5zm3 4a1 1 0 000 2h.01a1 1 0 100-2H7zm3 0a1 1 0 000 2h3a1 1 0 100-2h-3zm-3 4a1 1 0 100 2h.01a1 1 0 100-2H7zm3 0a1 1 0 100 2h3a1 1 0 100-2h-3z"
                clipRule="evenodd"
              />
            </svg>
            <span>Vector: {formatScore(result.vectorScore)}%</span>
            {result.vectorRank != null && (
              <span className="text-[var(--text-tertiary)]">#{result.vectorRank + 1}</span>
            )}
          </div>
        )}
        {result.bm25Score != null && (
          <div className="flex items-center gap-1 bg-[var(--success-light)]/30 text-[var(--success)] px-2 py-1 rounded">
            <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
              <path
                fillRule="evenodd"
                d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z"
                clipRule="evenodd"
              />
            </svg>
            <span>BM25: {formatScore(result.bm25Score)}%</span>
            {result.bm25Rank != null && (
              <span className="text-[var(--text-tertiary)]">#{result.bm25Rank + 1}</span>
            )}
          </div>
        )}
        {(result.metadata.updated_at ? (
          <div className="flex items-center gap-1 text-[var(--text-secondary)]" key="modified-at">
            <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
              <path
                fillRule="evenodd"
                d="M10 18a8 8 0 100-16 8 8 0 000 16zm1-12a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z"
                clipRule="evenodd"
              />
            </svg>
            <span>
              {new Date(result.metadata.updated_at as string).toLocaleDateString()}
            </span>
          </div>
        ) : null) as React.ReactNode}
      </div>
    </button>
  );
}

export const SearchResult = memo(SearchResultComponent);
