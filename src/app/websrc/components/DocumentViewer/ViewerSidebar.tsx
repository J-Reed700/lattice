import { memo, useMemo } from 'react';

import { Calendar, FileText, HardDrive, TrendingUp } from 'lucide-react';

import type { FileData } from './DocumentViewer';
import type { SearchResult } from '../../types';

/**
 * ViewerSidebar
 *
 * Purpose: Display file metadata and search relevance information
 *
 * Features:
 * - File information (type, size, dates)
 * - Search relevance scores
 * - File path
 * - Formatted display of metadata
 *
 * States: default
 * Accessibility: WCAG AA, semantic HTML
 * Performance: Optimized with React.memo and useMemo for expensive computations
 */

export interface ViewerSidebarProps {
  fileData: FileData;
  result: SearchResult;
}

export const ViewerSidebar = memo(({ fileData, result }: ViewerSidebarProps) => {
  // Memoize formatted values to prevent recalculation on every render
  const formattedSize = useMemo(() => {
    const bytes = fileData.sizeBytes;
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${Math.round((bytes / Math.pow(k, i)) * 100) / 100  } ${  sizes[i]}`;
  }, [fileData.sizeBytes]);

  const formattedDate = useMemo(() => {
    try {
      const date = new Date(fileData.modifiedAt);
      return date.toLocaleString(undefined, {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
      });
    } catch {
      return fileData.modifiedAt;
    }
  }, [fileData.modifiedAt]);

  const formatScore = (score: number): string => (score * 100).toFixed(1);

  return (
    <div className="w-96 bg-[hsl(var(--surface-raised))] border-l border-[hsl(var(--border-subtle))] overflow-y-auto flex-shrink-0">
      <div className="p-6 space-y-6">
        {/* Search Relevance Section */}
        <div>
          <h3 className="text-lg font-semibold mb-4 text-[hsl(var(--text-primary))] flex items-center gap-2">
            <TrendingUp className="w-5 h-5" />
            Search Relevance
          </h3>

          <div className="space-y-3">
            <div className="bg-[hsl(var(--surface))] rounded-lg p-3">
              <div className="flex items-center justify-between mb-1">
                <span className="text-sm font-medium text-[hsl(var(--text-secondary))]">
                  Overall Score
                </span>
                <span className="text-lg font-bold text-[hsl(var(--accent))]">
                  {formatScore(result.score)}%
                </span>
              </div>
              <div className="w-full bg-[hsl(var(--surface-raised))] rounded-full h-2 overflow-hidden">
                <div
                  className="bg-[hsl(var(--accent))] h-full transition-colors duration-300"
                  style={{ width: `${result.score * 100}%` }}
                  aria-label={`Relevance score: ${formatScore(result.score)}%`}
                />
              </div>
            </div>

            {result.vectorScore != null && (
              <div className="flex items-center justify-between text-sm">
                <span className="text-[hsl(var(--text-secondary))]">Semantic Match</span>
                <span className="font-semibold text-[hsl(var(--text-primary))]">
                  {formatScore(result.vectorScore)}%
                  {result.vectorRank != null && (
                    <span className="text-[hsl(var(--text-tertiary))] ml-1">
                      (#{result.vectorRank + 1})
                    </span>
                  )}
                </span>
              </div>
            )}

            {result.bm25Score != null && (
              <div className="flex items-center justify-between text-sm">
                <span className="text-[hsl(var(--text-secondary))]">Keyword Match</span>
                <span className="font-semibold text-[hsl(var(--text-primary))]">
                  {formatScore(result.bm25Score)}%
                  {result.bm25Rank != null && (
                    <span className="text-[hsl(var(--text-tertiary))] ml-1">
                      (#{result.bm25Rank + 1})
                    </span>
                  )}
                </span>
              </div>
            )}
          </div>
        </div>

        {/* File Information Section */}
        <div>
          <h3 className="text-lg font-semibold mb-4 text-[hsl(var(--text-primary))]">
            File Information
          </h3>

          <div className="space-y-3">
            <div className="flex items-start gap-3">
              <FileText className="w-5 h-5 text-[hsl(var(--text-secondary))] mt-0.5 flex-shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-[hsl(var(--text-secondary))] mb-1">Type</p>
                <p className="text-sm text-[hsl(var(--text-primary))] font-mono">
                  {fileData.fileType.toUpperCase()}
                </p>
              </div>
            </div>

            <div className="flex items-start gap-3">
              <HardDrive className="w-5 h-5 text-[hsl(var(--text-secondary))] mt-0.5 flex-shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-[hsl(var(--text-secondary))] mb-1">Size</p>
                <p className="text-sm text-[hsl(var(--text-primary))]">
                  {formattedSize}
                </p>
              </div>
            </div>

            <div className="flex items-start gap-3">
              <Calendar className="w-5 h-5 text-[hsl(var(--text-secondary))] mt-0.5 flex-shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-[hsl(var(--text-secondary))] mb-1">
                  Modified
                </p>
                <p className="text-sm text-[hsl(var(--text-primary))]">
                  {formattedDate}
                </p>
              </div>
            </div>
          </div>
        </div>

        {/* File Path Section */}
        <div>
          <h3 className="text-sm font-medium text-[hsl(var(--text-secondary))] mb-2">
            File Location
          </h3>
          <p className="text-xs text-[hsl(var(--text-secondary))] break-all font-mono bg-[hsl(var(--surface))] p-3 rounded-lg">
            {fileData.path}
          </p>
        </div>

        {/* Content Preview (if available) */}
        {result.content && (
          <div>
            <h3 className="text-sm font-medium text-[hsl(var(--text-secondary))] mb-2">
              Matching Content
            </h3>
            <div className="text-xs text-[hsl(var(--text-secondary))] bg-[hsl(var(--surface))] p-3 rounded-lg">
              <p className="line-clamp-6">{result.content}</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}, (prevProps, nextProps) => 
  // Only re-render if file path or result ID changes
   prevProps.fileData.path === nextProps.fileData.path &&
    prevProps.result.id === nextProps.result.id
);

ViewerSidebar.displayName = 'ViewerSidebar';

export default ViewerSidebar;
