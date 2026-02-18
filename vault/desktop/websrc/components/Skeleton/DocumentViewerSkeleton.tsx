/**
 * DocumentViewerSkeleton Component
 *
 * Purpose: Skeleton loader for document viewer while content loads
 *
 * Features:
 * - Header bar skeleton with actions
 * - Content area with paragraph placeholders
 * - Optional sidebar skeleton
 * - Matches DocumentViewer layout structure
 *
 * States: loading only
 * Accessibility: Hidden from screen readers
 * Performance: Minimal re-renders with memo
 */

import React, { memo } from 'react';

import { Skeleton, SkeletonText } from './Skeleton';

export interface DocumentViewerSkeletonProps {
  /** Whether to show sidebar skeleton */
  showSidebar?: boolean;

  /** Additional CSS classes */
  className?: string;
}

export const DocumentViewerSkeleton: React.FC<DocumentViewerSkeletonProps> = memo(({
  showSidebar = true,
  className = '',
}) => (
    <div
      className={`fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex flex-col ${className}`}
      role="status"
      aria-label="Loading document"
    >
      {/* Header Skeleton */}
      <DocumentViewerHeaderSkeleton />

      {/* Main Content Area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Document Content Skeleton */}
        <div className={`flex-1 overflow-hidden transition-all duration-300 ${showSidebar ? 'mr-96' : ''}`}>
          <DocumentContentSkeleton />
        </div>

        {/* Sidebar Skeleton */}
        {showSidebar && <DocumentSidebarSkeleton />}
      </div>

      <span className="sr-only">Loading document viewer...</span>
    </div>
  ));

DocumentViewerSkeleton.displayName = 'DocumentViewerSkeleton';

/**
 * Header bar skeleton
 */
const DocumentViewerHeaderSkeleton: React.FC = memo(() => (
    <div className="bg-[var(--surface-elevated)] border-b border-[var(--border-color)] px-6 py-4">
      <div className="flex items-center justify-between">
        {/* Left: File name */}
        <div className="flex items-center gap-3 flex-1 min-w-0">
          <Skeleton width={32} height={32} />
          <Skeleton height="1.5rem" width="300px" />
        </div>

        {/* Right: Action buttons */}
        <div className="flex items-center gap-2">
          <Skeleton width={40} height={40} className="rounded-lg" />
          <Skeleton width={40} height={40} className="rounded-lg" />
          <Skeleton width={40} height={40} className="rounded-lg" />
          <Skeleton width={40} height={40} className="rounded-lg" />
        </div>
      </div>
    </div>
  ));

DocumentViewerHeaderSkeleton.displayName = 'DocumentViewerHeaderSkeleton';

/**
 * Main document content skeleton
 */
const DocumentContentSkeleton: React.FC = memo(() => (
    <div className="h-full bg-[var(--bg-secondary)] p-8 overflow-auto">
      <div className="max-w-4xl mx-auto bg-[var(--surface-elevated)] rounded-lg shadow-lg p-8">
        {/* Document title area */}
        <div className="mb-8 space-y-3">
          <Skeleton height="2rem" width="80%" />
          <Skeleton height="1rem" width="40%" />
        </div>

        {/* Document content paragraphs */}
        <div className="space-y-6">
          {/* Paragraph 1 */}
          <div className="space-y-2">
            <SkeletonText lines={4} lastLineWidth="90%" />
          </div>

          {/* Paragraph 2 */}
          <div className="space-y-2">
            <SkeletonText lines={5} lastLineWidth="75%" />
          </div>

          {/* Image placeholder */}
          <Skeleton height="200px" width="100%" className="rounded-lg" />

          {/* Paragraph 3 */}
          <div className="space-y-2">
            <SkeletonText lines={4} lastLineWidth="85%" />
          </div>

          {/* Paragraph 4 */}
          <div className="space-y-2">
            <SkeletonText lines={3} lastLineWidth="95%" />
          </div>

          {/* Code block placeholder */}
          <div className="bg-[var(--bg-primary)] rounded-lg p-4 space-y-2">
            <Skeleton height="1rem" width="60%" />
            <Skeleton height="1rem" width="80%" />
            <Skeleton height="1rem" width="50%" />
            <Skeleton height="1rem" width="70%" />
          </div>

          {/* Paragraph 5 */}
          <div className="space-y-2">
            <SkeletonText lines={4} lastLineWidth="80%" />
          </div>
        </div>
      </div>
    </div>
  ));

DocumentContentSkeleton.displayName = 'DocumentContentSkeleton';

/**
 * Sidebar skeleton
 */
const DocumentSidebarSkeleton: React.FC = memo(() => (
    <div className="w-96 bg-[var(--surface-elevated)] border-l border-[var(--border-color)] p-6 overflow-auto flex-shrink-0">
      {/* File info section */}
      <div className="mb-6">
        <Skeleton height="1.25rem" width="120px" className="mb-4" />
        <div className="space-y-3">
          <InfoRowSkeleton />
          <InfoRowSkeleton />
          <InfoRowSkeleton />
          <InfoRowSkeleton />
          <InfoRowSkeleton />
        </div>
      </div>

      <div className="border-t border-[var(--border-color)] pt-6 mb-6">
        <Skeleton height="1.25rem" width="100px" className="mb-4" />
        <div className="space-y-3">
          <InfoRowSkeleton />
          <InfoRowSkeleton />
        </div>
      </div>

      {/* Actions section */}
      <div className="border-t border-[var(--border-color)] pt-6">
        <Skeleton height="1.25rem" width="80px" className="mb-4" />
        <div className="space-y-2">
          <Skeleton height="40px" width="100%" className="rounded-lg" />
          <Skeleton height="40px" width="100%" className="rounded-lg" />
          <Skeleton height="40px" width="100%" className="rounded-lg" />
        </div>
      </div>
    </div>
  ));

DocumentSidebarSkeleton.displayName = 'DocumentSidebarSkeleton';

/**
 * Info row skeleton (label + value)
 */
const InfoRowSkeleton: React.FC = () => (
    <div className="flex justify-between items-center">
      <Skeleton height="0.875rem" width="80px" />
      <Skeleton height="0.875rem" width="120px" />
    </div>
  );

/**
 * Minimal document viewer skeleton (without modal overlay)
 */
export const DocumentViewerSkeletonMinimal: React.FC = memo(() => (
    <div className="flex flex-col h-full bg-[var(--surface-elevated)]" role="status">
      {/* Header */}
      <div className="border-b border-[var(--border-color)] px-6 py-4">
        <div className="flex items-center gap-3">
          <Skeleton width={32} height={32} />
          <Skeleton height="1.5rem" width="250px" />
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 p-8 overflow-auto">
        <div className="max-w-3xl mx-auto space-y-4">
          <Skeleton height="2rem" width="70%" />
          <SkeletonText lines={6} />
          <Skeleton height="150px" width="100%" className="rounded-lg" />
          <SkeletonText lines={4} />
        </div>
      </div>

      <span className="sr-only">Loading document...</span>
    </div>
  ));

DocumentViewerSkeletonMinimal.displayName = 'DocumentViewerSkeletonMinimal';
