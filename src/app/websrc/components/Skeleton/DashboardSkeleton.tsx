/**
 * DashboardSkeleton Component
 *
 * Purpose: Skeleton loader that mimics the Dashboard layout
 *
 * Features:
 * - Matches Dashboard structure with stats, quick actions, and content grid
 * - Shows placeholders for statistics cards, recent documents, and activity
 * - Smooth shimmer animation
 * - Responsive layout matching actual dashboard
 *
 * States: loading only
 * Accessibility: Hidden from screen readers with proper ARIA labels
 * Performance: Optimized rendering with memoization
 */

import React, { memo } from 'react';

import { Skeleton, SkeletonCard } from './Skeleton';

export interface DashboardSkeletonProps {
  /** Additional CSS classes */
  className?: string;
}

export const DashboardSkeleton: React.FC<DashboardSkeletonProps> = memo(({
  className = '',
}) => (
    <div className={`space-y-6 ${className}`} role="status" aria-label="Loading dashboard">
      {/* Header Skeleton */}
      <div className="space-y-2">
        <Skeleton height="2.5rem" width="60%" className="mb-2" />
        <Skeleton height="1rem" width="40%" />
      </div>

      {/* Statistics Cards Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, index) => (
          <StatsCardSkeleton key={index} />
        ))}
      </div>

      {/* Quick Actions Skeleton */}
      <div className="flex flex-wrap gap-3">
        {Array.from({ length: 3 }).map((_, index) => (
          <Skeleton key={index} height="2.5rem" width="140px" className="rounded-lg" />
        ))}
      </div>

      {/* Main Content Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Recent Documents - Takes 2/3 width on large screens */}
        <div className="lg:col-span-2">
          <RecentDocumentsSkeleton />
        </div>

        {/* Recent Activity - Takes 1/3 width on large screens */}
        <div>
          <RecentActivitySkeleton />
        </div>
      </div>

      <span className="sr-only">Loading dashboard...</span>
    </div>
  ));

DashboardSkeleton.displayName = 'DashboardSkeleton';

/**
 * Individual stats card skeleton
 */
const StatsCardSkeleton: React.FC = memo(() => (
    <div
      className="bg-[var(--surface-elevated)] rounded-lg border border-[var(--border-color)] p-6 elevation-1"
      aria-hidden="true"
    >
      <div className="flex items-start justify-between mb-4">
        <Skeleton circle width={40} height={40} />
        <Skeleton height="1rem" width="60px" />
      </div>
      <Skeleton height="2rem" width="80%" className="mb-2" />
      <Skeleton height="0.875rem" width="50%" />
    </div>
  ));

StatsCardSkeleton.displayName = 'StatsCardSkeleton';

/**
 * Recent documents section skeleton
 */
const RecentDocumentsSkeleton: React.FC = memo(() => (
    <div
      className="bg-[var(--surface-elevated)] rounded-lg border border-[var(--border-color)] p-6 elevation-1"
      aria-hidden="true"
    >
      {/* Section Header */}
      <div className="flex items-center justify-between mb-6">
        <Skeleton height="1.5rem" width="180px" />
        <Skeleton height="1rem" width="80px" />
      </div>

      {/* Document List */}
      <div className="space-y-4">
        {Array.from({ length: 5 }).map((_, index) => (
          <div key={index} className="flex items-center gap-4 p-3 rounded-lg hover:bg-[var(--surface-hover)]/50">
            {/* File Icon */}
            <Skeleton width={40} height={40} className="rounded" />

            {/* File Info */}
            <div className="flex-1 min-w-0 space-y-2">
              <Skeleton height="1rem" width="70%" />
              <Skeleton height="0.75rem" width="50%" />
            </div>

            {/* File Size */}
            <Skeleton height="0.875rem" width="60px" />
          </div>
        ))}
      </div>
    </div>
  ));

RecentDocumentsSkeleton.displayName = 'RecentDocumentsSkeleton';

/**
 * Recent activity section skeleton
 */
const RecentActivitySkeleton: React.FC = memo(() => (
    <div
      className="bg-[var(--surface-elevated)] rounded-lg border border-[var(--border-color)] p-6 elevation-1"
      aria-hidden="true"
    >
      {/* Section Header */}
      <div className="mb-6">
        <Skeleton height="1.5rem" width="150px" />
      </div>

      {/* Activity List */}
      <div className="space-y-6">
        {Array.from({ length: 6 }).map((_, index) => (
          <div key={index} className="flex items-start gap-3">
            {/* Status Icon */}
            <Skeleton circle width={32} height={32} />

            {/* Activity Info */}
            <div className="flex-1 space-y-2">
              <Skeleton height="0.875rem" width="90%" />
              <Skeleton height="0.75rem" width="60%" />
            </div>
          </div>
        ))}
      </div>
    </div>
  ));

RecentActivitySkeleton.displayName = 'RecentActivitySkeleton';

/**
 * Compact version for smaller spaces
 */
export const DashboardSkeletonCompact: React.FC = memo(() => (
    <div className="space-y-4" role="status" aria-label="Loading dashboard">
      {/* Stats Cards - 2 columns on mobile */}
      <div className="grid grid-cols-2 gap-3">
        {Array.from({ length: 4 }).map((_, index) => (
          <div
            key={index}
            className="bg-[var(--surface-elevated)] rounded-lg border border-[var(--border-color)] p-4"
            aria-hidden="true"
          >
            <Skeleton circle width={32} height={32} className="mb-3" />
            <Skeleton height="1.5rem" width="70%" className="mb-2" />
            <Skeleton height="0.75rem" width="50%" />
          </div>
        ))}
      </div>

      {/* Content Placeholder */}
      <SkeletonCard className="h-64" />

      <span className="sr-only">Loading dashboard...</span>
    </div>
  ));

DashboardSkeletonCompact.displayName = 'DashboardSkeletonCompact';
