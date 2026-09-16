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

/**
 * Fused retrieval score, shown as the 0–1 number it actually is.
 * BM25 is an unbounded relevance score, not a percentage — it is printed raw
 * on the component line below.
 */
function formatScore(score: number): string {
  return score.toFixed(2);
}

function SearchResultComponent({ result, query, onOpen }: SearchResultProps) {
  const handleClick = useCallback(() => {
    onOpen?.(result);
  }, [onOpen, result]);

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
          <mark key={`${part}-${idx}`} className="bg-transparent font-medium text-text-primary">
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

  const components: string[] = [];
  if (result.vectorScore != null) {
    components.push(`vec ${result.vectorScore.toFixed(2)}`);
  }
  if (result.bm25Score != null) {
    components.push(`bm25 ${result.bm25Score.toFixed(1)}`);
  }

  return (
    <button
      type="button"
      className="w-full border-b border-border-subtle px-2 py-3 text-left transition-colors duration-fast hover:bg-surface"
      onClick={handleClick}
    >
      <div className="flex items-baseline gap-4">
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text-primary">{displayTitle}</div>
          {displayPath && (
            <div className="truncate text-xs text-text-muted">{displayPath}</div>
          )}
        </div>
        <div className="shrink-0 text-right">
          <div className="text-xs tabular-nums text-text-muted">{formatScore(result.score)}</div>
          {components.length > 0 && (
            <div className="font-mono text-xxs text-text-muted">{components.join(' · ')}</div>
          )}
        </div>
      </div>

      {highlightedExcerpts.length > 0 && (
        <div className="mt-1.5 space-y-1">
          {highlightedExcerpts.map((excerpt, index) => (
            <p
              className="line-clamp-2 text-sm text-text-secondary"
              key={`${result.id}-excerpt-${index}`}
            >
              {renderHighlightedText(excerpt)}
            </p>
          ))}
        </div>
      )}
    </button>
  );
}

export const SearchResult = memo(SearchResultComponent);
