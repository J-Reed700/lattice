/**
 * RecentDocumentsPanel Component
 *
 * Displays recently accessed documents with access metadata.
 * Features:
 * - Shows last N accessed documents
 * - Timestamps with relative time formatting
 * - Access count for analytics
 * - Clear all button
 * - Click to open document
 * - Empty state when no recent documents
 */

import React, { memo, useEffect, useCallback } from 'react';

import { Clock, ExternalLink, FolderOpen } from 'lucide-react';

import { useFavorites, type RecentDocument } from '../../contexts/FavoritesContext';
import VaultAPI from '../../lib/api';
import { EmptyState } from '../EmptyState';
import { LoadingState } from '../LoadingState';
import { useToast } from '../ui/Toast';

interface RecentDocumentsPanelProps {
  limit?: number;
  onDocumentClick?: (_documentId: string, _documentPath: string) => void;
  className?: string;
}

export const RecentDocumentsPanel = memo(({
  limit = 10,
  onDocumentClick,
  className = '',
}: RecentDocumentsPanelProps) => {
  const { recentDocs, isLoading, error, loadRecentDocs, clearRecentDocs } = useFavorites();
  const { addToast } = useToast();

  // Load recent documents on mount and when limit changes
  useEffect(() => {
    loadRecentDocs(limit);
  }, [loadRecentDocs, limit]);

  const handleDocumentClick = useCallback(
    (doc: RecentDocument) => {
      if (onDocumentClick) {
        onDocumentClick(doc.document_id, doc.document_path);
      }
    },
    [onDocumentClick]
  );

  const handleShowInFolder = useCallback(
    async (e: React.MouseEvent, path: string) => {
      e.stopPropagation();
      const result = await VaultAPI.showInFolder(path);

      if (!result.ok) {
        addToast('Failed to show in folder', 'error', 3000);
      }
    },
    [addToast]
  );

  const handleClearAll = useCallback(async () => {
    try {
      await clearRecentDocs();
      addToast('Recent documents cleared', 'success', 2000);
    } catch {
      addToast('Failed to clear recent documents', 'error', 3000);
    }
  }, [clearRecentDocs, addToast]);

  const formatRelativeTime = (dateString: string) => {
    try {
      const date = new Date(dateString);
      const now = new Date();
      const diffMs = now.getTime() - date.getTime();
      const diffSecs = Math.floor(diffMs / 1000);
      const diffMins = Math.floor(diffMs / 60000);
      const diffHours = Math.floor(diffMs / 3600000);
      const diffDays = Math.floor(diffMs / 86400000);

      if (diffSecs < 10) return 'Just now';
      if (diffSecs < 60) return `${diffSecs} seconds ago`;
      if (diffMins < 60) return `${diffMins} minute${diffMins > 1 ? 's' : ''} ago`;
      if (diffHours < 24) return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`;
      if (diffDays < 7) return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`;
      if (diffDays < 30) {
        const weeks = Math.floor(diffDays / 7);
        return `${weeks} week${weeks > 1 ? 's' : ''} ago`;
      }

      return date.toLocaleDateString();
    } catch {
      return dateString;
    }
  };

  if (isLoading) {
    return (
      <div className={`p-4 ${className}`}>
        <LoadingState size="sm" message="Loading recent documents..." />
      </div>
    );
  }

  if (error) {
    return (
      <div className={`p-4 ${className}`}>
        <EmptyState
          icon={<Clock />}
          title="Error loading recent documents"
          description={error}
          action={{
            label: 'Retry',
            onClick: () => loadRecentDocs(limit),
          }}
        />
      </div>
    );
  }

  if (recentDocs.length === 0) {
    return (
      <div className={`p-4 ${className}`}>
        <EmptyState
          icon={<Clock />}
          title="No recent documents"
          description="Documents you access will appear here for quick access"
        />
      </div>
    );
  }

  return (
    <>
      <div className={`flex flex-col ${className}`}>
        {/* Header */}
        <div className="px-4 py-3 border-b border-[hsl(var(--border-subtle))]">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Clock className="w-5 h-5 text-[hsl(var(--accent))]" />
              <h2 className="text-lg font-semibold text-[hsl(var(--text-primary))]">
                Recent Documents
              </h2>
              <span className="text-sm text-[hsl(var(--text-secondary))]">
                ({recentDocs.length})
              </span>
            </div>
            {recentDocs.length > 0 && (
              <button
                onClick={handleClearAll}
                className="px-3 py-1.5 text-sm font-medium text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] rounded-md transition-colors"
                title="Clear all recent documents"
              >
                Clear All
              </button>
            )}
          </div>
        </div>

        {/* Recent Documents List */}
        <div className="flex-1 overflow-y-auto">
          <div className="divide-y divide-gray-200">
            {recentDocs.map((doc) => (
              <div
                key={doc.id}
                className="group px-4 py-3 hover:bg-[hsl(var(--surface))] cursor-pointer transition-colors"
                onClick={() => handleDocumentClick(doc)}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    {/* Document name */}
                    <div className="flex items-center gap-2 mb-1">
                      <h3 className="text-sm font-medium text-[hsl(var(--text-primary))] truncate">
                        {doc.document_name}
                      </h3>
                      <ExternalLink className="w-3.5 h-3.5 text-[hsl(var(--text-tertiary))] opacity-0 group-hover:opacity-100 transition-opacity" />
                    </div>

                    {/* File type, time, and access count */}
                    <div className="flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
                      {doc.file_type && (
                        <span className="uppercase font-mono">{doc.file_type}</span>
                      )}
                      {doc.file_type && <span>•</span>}
                      <span>{formatRelativeTime(doc.last_accessed_at)}</span>
                      <span>•</span>
                      <span title={`Accessed ${doc.access_count} time${doc.access_count > 1 ? 's' : ''}`}>
                        {doc.access_count} view{doc.access_count > 1 ? 's' : ''}
                      </span>
                    </div>

                    {/* Path (truncated) */}
                    <div className="mt-1 text-xs text-[hsl(var(--text-tertiary))] truncate">
                      {doc.document_path}
                    </div>
                  </div>

                  {/* Actions */}
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onClick={(e) => handleShowInFolder(e, doc.document_path)}
                      className="p-1.5 rounded hover:bg-[hsl(var(--surface-raised))] transition-colors"
                      title="Show in folder"
                      aria-label="Show in folder"
                    >
                      <FolderOpen className="w-4 h-4 text-[hsl(var(--text-secondary))]" />
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

    </>
  );
});
