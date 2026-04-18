import { memo, useMemo, useCallback } from 'react';

import { sanitizeFileName } from '@/utils/sanitize';

import { SearchResultSkeleton } from '../Skeleton';
import Card from '../ui/Card/Card';

import type { SearchResult } from '../../types';


/**
 * ResultsList
 *
 * Purpose: Display search results with rich metadata and interaction
 *
 * Features:
 * - Virtualized list for performance (future enhancement)
 * - Click to open document
 * - Relevance score visualization
 * - File type icons
 * - Content snippet with highlighting
 * - Loading skeleton states
 * - Empty state with helpful messaging
 *
 * States: loading, empty, error, populated
 * Accessibility: WCAG AA, keyboard navigation, semantic HTML
 */

interface ResultsListProps {
  results: SearchResult[];
  isLoading?: boolean;
  error?: string | null;
  onResultClick?: (result: SearchResult) => void;
  emptyMessage?: string;
}

export const ResultsList = memo(({
  results,
  isLoading = false,
  error = null,
  onResultClick,
  emptyMessage = 'No results found',
}: ResultsListProps) => {
  // Memoize results to prevent unnecessary re-renders
  const sortedResults = useMemo(
    () => [...results].sort((a, b) => b.score - a.score),
    [results]
  );

  // Memoize callback to prevent child re-renders
  const handleResultClick = useCallback(
    (result: SearchResult) => {
      onResultClick?.(result);
    },
    [onResultClick]
  );
  // Loading state
  if (isLoading) {
    return <SearchResultSkeleton count={5} />;
  }

  // Error state
  if (error) {
    return (
      <div
        className="flex flex-col items-center justify-center py-12 px-4"
        role="alert"
        aria-live="polite"
      >
        <div className="w-16 h-16 bg-[hsl(var(--danger-muted))]/30 rounded-full flex items-center justify-center mb-4">
          <svg
            className="w-8 h-8 text-[hsl(var(--danger-fg))]"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
        </div>
        <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
          Search Error
        </h3>
        <p className="text-sm text-[hsl(var(--text-secondary))] text-center max-w-md">
          {error}
        </p>
      </div>
    );
  }

  // Empty state
  if (results.length === 0) {
    return (
      <div
        className="flex flex-col items-center justify-center py-12 px-4"
        role="status"
        aria-live="polite"
      >
        <div className="w-16 h-16 bg-[hsl(var(--bg))] rounded-full flex items-center justify-center mb-4">
          <svg
            className="w-8 h-8 text-[hsl(var(--text-tertiary))]"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
            />
          </svg>
        </div>
        <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
          {emptyMessage}
        </h3>
        <p className="text-sm text-[hsl(var(--text-secondary))] text-center max-w-md">
          Try a different search term or check your search mode
        </p>
      </div>
    );
  }

  // Results list
  return (
    <div
      className="space-y-4"
      role="list"
      aria-label={`${sortedResults.length} search results`}
    >
      {sortedResults.map((result, index) => (
        <ResultItem
          key={result.id}
          result={result}
          index={index}
          onClick={handleResultClick}
        />
      ))}
    </div>
  );
});

// Individual result item component
interface ResultItemProps {
  result: SearchResult;
  index: number;
  onClick: (result: SearchResult) => void;
}

// Type-safe metadata accessors
function getMetadataString(metadata: Record<string, unknown>, key: string): string | undefined {
  const value = metadata[key];
  return typeof value === 'string' ? value : undefined;
}

const ResultItem = memo(({ result, index, onClick }: ResultItemProps) => {
  // Use direct fields FIRST (from Rust SearchResultDto), fallback to metadata if needed
  const fileName = sanitizeFileName(result.title || getMetadataString(result.metadata, 'filename') || result.id);
  const filePath = result.path || getMetadataString(result.metadata, 'path');
  const fileType = getMetadataString(result.metadata, 'file_type')?.toUpperCase() || 'FILE';
  const modifiedAt = getMetadataString(result.metadata, 'updated_at');

  const handleClick = useCallback(() => {
    onClick(result);
  }, [result, onClick]);

  return (
    <Card
      variant="clickable"
      onClick={handleClick}
      role="listitem"
      aria-label={`Result ${index + 1}: ${fileName}`}
    >
      {/* Header: File name and score */}
      <div className="flex items-start justify-between mb-3">
        <div className="flex-1 min-w-0">
          <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-1 truncate">
            {fileName}
          </h3>
          {filePath && (
            <p className="text-xs text-[hsl(var(--text-secondary))] truncate" title={filePath}>
              {filePath}
            </p>
          )}
        </div>

        <div className="ml-4 flex flex-col items-end flex-shrink-0">
          <ScoreBadge score={result.score} />
          <span className="text-xs text-[hsl(var(--text-secondary))] mt-1">
            {fileType}
          </span>
        </div>
      </div>

      {/* Content snippet */}
      {result.content && (
        <div className="mb-3">
          <p className="text-sm text-[hsl(var(--text-secondary))] line-clamp-3">
            {result.content}
          </p>
        </div>
      )}

      {/* Metadata footer */}
      <div className="flex flex-wrap gap-2 items-center">
        {/* Vector score */}
        {result.vectorScore != null && (
          <MetadataBadge
            icon={
              <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                <path d="M9 2a1 1 0 000 2h2a1 1 0 100-2H9z" />
                <path
                  fillRule="evenodd"
                  d="M4 5a2 2 0 012-2 3 3 0 003 3h2a3 3 0 003-3 2 2 0 012 2v11a2 2 0 01-2 2H6a2 2 0 01-2-2V5zm3 4a1 1 0 000 2h.01a1 1 0 100-2H7zm3 0a1 1 0 000 2h3a1 1 0 100-2h-3zm-3 4a1 1 0 100 2h.01a1 1 0 100-2H7zm3 0a1 1 0 100 2h3a1 1 0 100-2h-3z"
                  clipRule="evenodd"
                />
              </svg>
            }
            color="blue"
            label={`Vector: ${formatScore(result.vectorScore)}%`}
            badge={result.vectorRank != null ? `#${result.vectorRank + 1}` : undefined}
          />
        )}

        {/* BM25 score */}
        {result.bm25Score != null && (
          <MetadataBadge
            icon={
              <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                <path
                  fillRule="evenodd"
                  d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z"
                  clipRule="evenodd"
                />
              </svg>
            }
            color="green"
            label={`BM25: ${formatScore(result.bm25Score)}%`}
            badge={result.bm25Rank != null ? `#${result.bm25Rank + 1}` : undefined}
          />
        )}

        {/* Modified date */}
        {modifiedAt && (
          <MetadataBadge
            icon={
              <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                <path
                  fillRule="evenodd"
                  d="M10 18a8 8 0 100-16 8 8 0 000 16zm1-12a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z"
                  clipRule="evenodd"
                />
              </svg>
            }
            color="gray"
            label={new Date(modifiedAt).toLocaleDateString()}
          />
        )}
      </div>
    </Card>
  );
});

ResultItem.displayName = 'ResultItem';

// Score badge component (memoized for performance)
const ScoreBadge = memo(({ score }: { score: number }) => {
  const percentage = formatScore(score);
  const color = getScoreColor(score);

  return (
    <div
      className={`text-sm font-semibold ${color}`}
      aria-label={`Relevance score: ${percentage}%`}
    >
      {percentage}%
    </div>
  );
});

ScoreBadge.displayName = 'ScoreBadge';

// Metadata badge component
interface MetadataBadgeProps {
  icon: React.ReactNode;
  color: 'blue' | 'green' | 'gray';
  label: string;
  badge?: string;
}

const MetadataBadge = memo(({ icon, color, label, badge }: MetadataBadgeProps) => {
  const colorStyles = {
    blue: 'bg-[hsl(var(--accent-muted))]/30 text-[hsl(var(--accent))]',
    green: 'bg-[hsl(var(--success-muted))]/30 text-[hsl(var(--success-fg))]',
    gray: 'bg-[hsl(var(--bg))] text-[hsl(var(--text-secondary))]',
  };

  return (
    <div className={`flex items-center gap-1 px-2 py-1 rounded text-xs ${colorStyles[color]}`}>
      {icon}
      <span>{label}</span>
      {badge && (
        <span className="text-[hsl(var(--text-tertiary))] font-medium">{badge}</span>
      )}
    </div>
  );
});

MetadataBadge.displayName = 'MetadataBadge';

// Utility functions
function formatScore(score: number): string {
  return (score * 100).toFixed(1);
}

function getScoreColor(score: number): string {
  if (score >= 0.8) return 'text-[hsl(var(--success-fg))]';
  if (score >= 0.6) return 'text-[hsl(var(--warning-fg))]';
  return 'text-[hsl(var(--text-secondary))]';
}
