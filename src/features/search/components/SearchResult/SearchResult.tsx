import { memo, useCallback, useMemo, type ReactNode } from 'react';

import { Eye } from 'lucide-react';

import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui';
import { cn } from '@/lib/utils';
import type { SearchResult as SearchResultType } from '@/types';

import {
  cleanSearchPath,
  describeSearchLocation,
  inferSearchResultType,
} from '../SearchInterface/searchFacets';

interface SearchResultProps {
  result: SearchResultType;
  query?: string;
  isSelected?: boolean;
  showQuickPreviewAction?: boolean;
  onSelect?: (result: SearchResultType) => void;
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

export function selectInformativeTerms(terms: string[], maxTerms: number): string[] {
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

export function getQueryTerms(query?: string): string[] {
  if (!query) return [];
  const terms = query
    .split(/\s+/)
    .map((term) => term.trim().toLowerCase());
  return selectInformativeTerms(terms, 10);
}

export function renderHighlightedExcerpt(text: string, queryTerms: string[]): ReactNode {
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
        <mark key={`${part}-${idx}`} className="text-text-primary">
          {part}
        </mark>
      );
    }
    return <span key={`${part}-${idx}`}>{part}</span>;
  });
}

export function getSearchResultDisplayMeta(result: SearchResultType) {
  const displayTitle = (result.title && result.title !== result.id)
    ? result.title
    : (result.metadata.filename as string || result.id);
  const rawPath = result.path || (result.metadata.path as string | undefined);
  const displayPath = cleanSearchPath(rawPath) ?? undefined;
  const location = describeSearchLocation(rawPath);
  const typeLabel = inferSearchResultType(result);

  const scoreComponents: string[] = [];
  if (result.vectorScore != null) {
    scoreComponents.push(`vec ${result.vectorScore.toFixed(2)}`);
  }
  if (result.bm25Score != null) {
    scoreComponents.push(`bm25 ${result.bm25Score.toFixed(1)}`);
  }

  const relevance = Math.max(0, Math.min(1, result.score));

  return {
    displayTitle,
    rawPath,
    displayPath,
    location,
    typeLabel,
    scoreComponents,
    relevance,
    formattedScore: result.score.toFixed(2),
  };
}

export function RelevanceMeter({
  score,
  formattedScore,
  components,
}: {
  score: number;
  formattedScore: string;
  components: string[];
}) {
  const relevance = Math.max(0, Math.min(1, score));
  const detailText = components.join(' · ');

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className="inline-flex items-center gap-2 rounded-md px-1.5 py-0.5 transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.05)]"
          aria-label={detailText ? `Match score ${formattedScore} (${detailText})` : `Match score ${formattedScore}`}
        >
          <span
            aria-hidden="true"
            className="h-1.5 w-11 overflow-hidden rounded-full bg-[hsl(var(--text-primary)/0.09)]"
          >
            <span
              className="block h-full rounded-full bg-accent transition-[width] duration-base"
              style={{ width: `${Math.max(8, relevance * 100)}%` }}
            />
          </span>
          <span className="w-7 text-right text-xs tabular-nums text-text-secondary">
            {formattedScore}
          </span>
          {detailText && <span className="sr-only">{detailText}</span>}
        </span>
      </TooltipTrigger>
      <TooltipContent side="left" className="space-y-0.5 px-2.5 py-1.5">
        <div className="text-[11px] font-medium text-text-primary tabular-nums">
          Match relevance {formattedScore}
        </div>
        {detailText && (
          <div className="font-mono text-[10.5px] text-text-secondary tabular-nums">
            {detailText}
          </div>
        )}
      </TooltipContent>
    </Tooltip>
  );
}

function SearchResultComponent({
  result,
  query,
  isSelected = false,
  showQuickPreviewAction = false,
  onSelect,
  onOpen,
}: SearchResultProps) {
  const handleClick = useCallback(() => {
    onSelect?.(result);
    onOpen?.(result);
  }, [onOpen, onSelect, result]);

  const handleSelect = useCallback(() => {
    onSelect?.(result);
  }, [onSelect, result]);

  const highlightedExcerpts = useMemo(() => {
    const baseExcerpts = (result.highlights && result.highlights.length > 0)
      ? result.highlights
      : (result.content ? [result.content] : []);

    const trimmed = baseExcerpts
      .map((excerpt) => excerpt.trim())
      .filter((excerpt) => excerpt.length > 0);

    return trimmed.slice(0, 3);
  }, [result.content, result.highlights]);

  const queryTerms = useMemo(() => getQueryTerms(query), [query]);
  const meta = useMemo(() => getSearchResultDisplayMeta(result), [result]);

  return (
    <div
      className={cn(
        'group relative border-b border-border-subtle transition-colors duration-fast',
        isSelected
          ? 'bg-surface'
          : 'hover:bg-[hsl(var(--text-primary)/0.025)]'
      )}
      onMouseEnter={handleSelect}
    >
      {isSelected && (
        <span
          aria-hidden="true"
          className="absolute inset-y-2.5 left-0 w-0.5 rounded-full bg-accent"
        />
      )}
      <div className="flex items-start gap-1">
        <button
          type="button"
          className="min-w-0 flex-1 rounded-xl px-3.5 py-3.5 text-left focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
          onClick={handleClick}
          onFocus={handleSelect}
        >
          <div className="flex items-start gap-3">
            <div className="min-w-0 flex-1">
              <div className="truncate text-[15px] font-medium tracking-[-0.005em] text-text-primary">
                {meta.displayTitle}
              </div>
              <div className="mt-0.5 flex items-center gap-1.5 text-xs text-text-muted">
                <span className="inline-flex shrink-0 items-center rounded bg-[hsl(var(--text-primary)/0.06)] px-1.5 py-px text-[10.5px] font-medium text-text-secondary">
                  {meta.typeLabel}
                </span>
                {meta.location && (
                  <span className="truncate" title={meta.rawPath}>
                    {meta.location}
                  </span>
                )}
              </div>
            </div>
            <div className="shrink-0 pt-0.5">
              <RelevanceMeter
                score={meta.relevance}
                formattedScore={meta.formattedScore}
                components={meta.scoreComponents}
              />
            </div>
          </div>

          {highlightedExcerpts.length > 0 && (
            <div className="mt-2.5 space-y-1.5">
              {highlightedExcerpts.map((excerpt, index) => (
                <p
                  className="line-clamp-2 border-l-2 border-border-default pl-3 font-serif text-[14.5px] leading-relaxed text-text-secondary"
                  key={`${result.id}-excerpt-${index}`}
                >
                  {renderHighlightedExcerpt(excerpt, queryTerms)}
                </p>
              ))}
            </div>
          )}
        </button>

        {showQuickPreviewAction && onSelect && (
          <div className="shrink-0 pr-2 pt-3">
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-label="Inspect in quick preview"
                  aria-pressed={isSelected}
                  onClick={(e) => {
                    e.stopPropagation();
                    onSelect(result);
                  }}
                  className={cn(
                    'flex h-7 w-7 items-center justify-center rounded-md text-text-tertiary transition-[opacity,background-color,color] duration-fast hover:bg-[hsl(var(--text-primary)/0.07)] hover:text-text-primary focus-visible:opacity-100 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                    isSelected ? 'opacity-100 text-accent' : 'opacity-0 group-hover:opacity-100'
                  )}
                >
                  <Eye className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="top">Quick preview</TooltipContent>
            </Tooltip>
          </div>
        )}
      </div>
    </div>
  );
}

export const SearchResult = memo(SearchResultComponent);
