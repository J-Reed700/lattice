/**
 * SearchResultSkeleton Component
 *
 * Purpose: Skeleton loader that mimics the SearchResultItem layout
 *
 * Features:
 * - Matches SearchResult card structure
 * - Shows title, snippet, and metadata placeholders
 * - Configurable number of result items
 * - Smooth shimmer animation
 *
 * States: loading only
 * Accessibility: Hidden from screen readers
 * Performance: Optimized rendering with memoization
 */

import React, { memo } from 'react';

import { Skeleton, SkeletonText } from './Skeleton';

export interface SearchResultSkeletonProps {
  /** Number of result skeletons to show */
  count?: number;

  /** Additional CSS classes */
  className?: string;
}

export const SearchResultSkeleton: React.FC<SearchResultSkeletonProps> = memo(({
  count = 5,
  className = '',
}) => (
    <div className={`space-y-4 ${className}`} role="status" aria-label="Loading search results">
      {Array.from({ length: count }).map((_, index) => (
        <SearchResultSkeletonItem key={index} />
      ))}
      <span className="sr-only">Loading search results...</span>
    </div>
  ));

SearchResultSkeleton.displayName = 'SearchResultSkeleton';

/**
 * Individual search result skeleton item
 */
const SearchResultSkeletonItem: React.FC = memo(() => (
    <div
      className="bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] p-6 transition-colors"
      aria-hidden="true"
    >
      {/* Header: File name and score */}
      <div className="flex items-start justify-between mb-3">
        <div className="flex-1 min-w-0 space-y-2">
          {/* File name */}
          <Skeleton height="1.25rem" width="80%" />
          {/* File path */}
          <Skeleton height="0.75rem" width="60%" />
        </div>

        {/* Score and file type */}
        <div className="ml-4 flex flex-col items-end flex-shrink-0 space-y-2">
          <Skeleton height="1rem" width="48px" />
          <Skeleton height="0.75rem" width="40px" />
        </div>
      </div>

      {/* Content snippet - 3 lines of text */}
      <div className="mb-3">
        <SkeletonText lines={3} lastLineWidth="85%" />
      </div>

      {/* Metadata footer - badges */}
      <div className="flex flex-wrap gap-2 items-center">
        <Skeleton height="24px" width="100px" className="rounded-full" />
        <Skeleton height="24px" width="100px" className="rounded-full" />
        <Skeleton height="24px" width="80px" className="rounded-full" />
      </div>
    </div>
  ));

SearchResultSkeletonItem.displayName = 'SearchResultSkeletonItem';

/**
 * Compact version for smaller spaces
 */
export const SearchResultSkeletonCompact: React.FC<{ count?: number }> = memo(({
  count = 3,
}) => (
    <div className="space-y-2" role="status" aria-label="Loading search results">
      {Array.from({ length: count }).map((_, index) => (
        <div
          key={index}
          className="bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] p-4"
          aria-hidden="true"
        >
          <div className="flex items-center gap-3">
            {/* Icon placeholder */}
            <Skeleton width={32} height={32} className="rounded" />

            {/* Content */}
            <div className="flex-1 space-y-2">
              <Skeleton height="1rem" width="70%" />
              <Skeleton height="0.75rem" width="50%" />
            </div>

            {/* Score */}
            <Skeleton height="1rem" width="40px" />
          </div>
        </div>
      ))}
      <span className="sr-only">Loading search results...</span>
    </div>
  ));

SearchResultSkeletonCompact.displayName = 'SearchResultSkeletonCompact';
