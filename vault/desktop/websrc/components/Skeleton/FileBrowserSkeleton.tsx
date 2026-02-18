/**
 * FileBrowserSkeleton Component
 *
 * Purpose: Skeleton loader for file browser tree structure
 *
 * Features:
 * - Hierarchical tree structure skeleton
 * - Nested folder indicators with indentation
 * - File and folder icons placeholder
 * - Configurable number of items
 *
 * States: loading only
 * Accessibility: Hidden from screen readers
 * Performance: Optimized with memo
 */

import React, { memo } from 'react';

import { Skeleton } from './Skeleton';

export interface FileBrowserSkeletonProps {
  /** Number of file/folder items to show */
  count?: number;

  /** Additional CSS classes */
  className?: string;

  /** View type */
  view?: 'tree' | 'list' | 'grid';
}

export const FileBrowserSkeleton: React.FC<FileBrowserSkeletonProps> = memo(({
  count = 10,
  className = '',
  view = 'tree',
}) => {
  if (view === 'grid') {
    return <FileBrowserGridSkeleton count={count} className={className} />;
  }

  if (view === 'list') {
    return <FileBrowserListSkeleton count={count} className={className} />;
  }

  return <FileBrowserTreeSkeleton count={count} className={className} />;
});

FileBrowserSkeleton.displayName = 'FileBrowserSkeleton';

/**
 * Tree view skeleton with nested structure
 */
const FileBrowserTreeSkeleton: React.FC<{ count: number; className: string }> = memo(({
  count,
  className,
}) => (
    <div className={`p-2 ${className}`} role="status" aria-label="Loading file tree">
      <div className="space-y-1">
        {/* Root folder */}
        <TreeItemSkeleton depth={0} isFolder />

        {/* Level 1 items */}
        <TreeItemSkeleton depth={1} isFolder />
        <TreeItemSkeleton depth={2} />
        <TreeItemSkeleton depth={2} />
        <TreeItemSkeleton depth={2} isFolder />

        {/* Level 2 items */}
        <TreeItemSkeleton depth={3} />
        <TreeItemSkeleton depth={3} />

        {/* More level 1 items */}
        <TreeItemSkeleton depth={1} />
        <TreeItemSkeleton depth={1} isFolder />
        <TreeItemSkeleton depth={2} />
        <TreeItemSkeleton depth={2} />

        {/* Additional items based on count */}
        {Array.from({ length: Math.max(0, count - 11) }).map((_, i) => (
          <TreeItemSkeleton
            key={i}
            depth={i % 3 === 0 ? 1 : i % 2 === 0 ? 2 : 0}
            isFolder={i % 4 === 0}
          />
        ))}
      </div>
      <span className="sr-only">Loading file browser...</span>
    </div>
  ));

FileBrowserTreeSkeleton.displayName = 'FileBrowserTreeSkeleton';

/**
 * Individual tree item skeleton
 */
interface TreeItemSkeletonProps {
  depth: number;
  isFolder?: boolean;
}

const TreeItemSkeleton: React.FC<TreeItemSkeletonProps> = memo(({ depth, isFolder = false }) => {
  const paddingLeft = depth * 20 + 8;

  return (
    <div
      className="flex items-center gap-2 px-2 py-1.5 rounded-md"
      style={{ paddingLeft: `${paddingLeft}px` }}
      aria-hidden="true"
    >
      {/* Chevron/expand indicator */}
      {isFolder && <Skeleton width={16} height={16} />}

      {/* File/folder icon */}
      <Skeleton width={16} height={16} />

      {/* File name */}
      <Skeleton
        height="0.875rem"
        width={`${60 + Math.random() * 40}%`}
        className="flex-1 max-w-xs"
      />

      {/* Optional badge/indicator */}
      {Math.random() > 0.7 && <Skeleton width={8} height={8} circle />}
    </div>
  );
});

TreeItemSkeleton.displayName = 'TreeItemSkeleton';

/**
 * List view skeleton
 */
const FileBrowserListSkeleton: React.FC<{ count: number; className: string }> = memo(({
  count,
  className,
}) => (
    <div className={`space-y-1 ${className}`} role="status" aria-label="Loading file list">
      {Array.from({ length: count }).map((_, index) => (
        <ListItemSkeleton key={index} />
      ))}
      <span className="sr-only">Loading file list...</span>
    </div>
  ));

FileBrowserListSkeleton.displayName = 'FileBrowserListSkeleton';

/**
 * Individual list item skeleton
 */
const ListItemSkeleton: React.FC = memo(() => (
    <div
      className="flex items-center gap-3 px-4 py-3 hover:bg-[var(--surface-hover)]/50 rounded-lg transition-colors"
      aria-hidden="true"
    >
      {/* Icon */}
      <Skeleton width={32} height={32} className="rounded" />

      {/* File info */}
      <div className="flex-1 min-w-0 space-y-1">
        <Skeleton height="1rem" width={`${50 + Math.random() * 30}%`} />
        <Skeleton height="0.75rem" width="120px" />
      </div>

      {/* Size/metadata */}
      <div className="flex items-center gap-4">
        <Skeleton height="0.875rem" width="60px" />
        <Skeleton height="0.875rem" width="80px" />
      </div>
    </div>
  ));

ListItemSkeleton.displayName = 'ListItemSkeleton';

/**
 * Grid view skeleton
 */
const FileBrowserGridSkeleton: React.FC<{ count: number; className: string }> = memo(({
  count,
  className,
}) => (
    <div
      className={`grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4 p-4 ${className}`}
      role="status"
      aria-label="Loading file grid"
    >
      {Array.from({ length: count }).map((_, index) => (
        <GridItemSkeleton key={index} />
      ))}
      <span className="sr-only">Loading file grid...</span>
    </div>
  ));

FileBrowserGridSkeleton.displayName = 'FileBrowserGridSkeleton';

/**
 * Individual grid item skeleton
 */
const GridItemSkeleton: React.FC = memo(() => (
    <div
      className="flex flex-col items-center p-4 rounded-lg border border-[var(--border-color)] hover:bg-[var(--surface-hover)]/50 transition-colors"
      aria-hidden="true"
    >
      {/* File icon/thumbnail */}
      <Skeleton width={64} height={64} className="rounded-lg mb-3" />

      {/* File name */}
      <Skeleton height="0.875rem" width="90%" className="mb-1" />
      <Skeleton height="0.75rem" width="60%" />
    </div>
  ));

GridItemSkeleton.displayName = 'GridItemSkeleton';

/**
 * Compact file browser skeleton for sidebars
 */
export const FileBrowserSkeletonCompact: React.FC = memo(() => (
    <div className="space-y-1" role="status" aria-label="Loading files">
      {[0, 1, 1, 2, 2, 1, 1].map((depth, index) => (
        <div
          key={index}
          className="flex items-center gap-2 px-2 py-1"
          style={{ paddingLeft: `${depth * 16 + 8}px` }}
          aria-hidden="true"
        >
          <Skeleton width={14} height={14} />
          <Skeleton height="0.75rem" width={`${50 + Math.random() * 30}%`} />
        </div>
      ))}
      <span className="sr-only">Loading files...</span>
    </div>
  ));

FileBrowserSkeletonCompact.displayName = 'FileBrowserSkeletonCompact';
