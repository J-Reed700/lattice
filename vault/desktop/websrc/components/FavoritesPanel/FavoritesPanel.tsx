/**
 * FavoritesPanel Component
 *
 * Displays a list of user's favorite documents in a sidebar panel.
 * Features:
 * - Shows favorited documents with metadata
 * - Click to open document
 * - Hover actions: Remove from favorites, Open in folder
 * - Empty state when no favorites
 * - Loading and error states
 */

import React, { memo, useCallback } from 'react';

import { Star, ExternalLink, Trash2, FolderOpen } from 'lucide-react';

import { useFavorites, type FavoriteDocument } from '../../contexts/FavoritesContext';
import VaultAPI from '../../lib/api';
import { EmptyState } from '../EmptyState';
import { LoadingState } from '../LoadingState';
import { useToast } from '../ui/Toast';

interface FavoritesPanelProps {
  onDocumentClick?: (documentId: string, documentPath: string) => void;
  className?: string;
}

export const FavoritesPanel = memo(({
  onDocumentClick,
  className = '',
}: FavoritesPanelProps) => {
  const { favoriteDocuments, isLoading, error, loadFavorites, removeFavorite } = useFavorites();
  const { addToast } = useToast();

  const handleDocumentClick = useCallback(
    (doc: FavoriteDocument) => {
      if (onDocumentClick) {
        onDocumentClick(doc.document_id, doc.document_path);
      }
    },
    [onDocumentClick]
  );

  const handleRemoveFavorite = useCallback(
    async (e: React.MouseEvent, docId: string) => {
      e.stopPropagation();
      try {
        await removeFavorite(docId);
        addToast('Removed from favorites', 'success', 2000);
      } catch {
        addToast('Failed to remove favorite', 'error', 3000);
      }
    },
    [removeFavorite, addToast]
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

  const formatDate = (dateString: string) => {
    try {
      const date = new Date(dateString);
      const now = new Date();
      const diffMs = now.getTime() - date.getTime();
      const diffMins = Math.floor(diffMs / 60000);
      const diffHours = Math.floor(diffMs / 3600000);
      const diffDays = Math.floor(diffMs / 86400000);

      if (diffMins < 1) return 'Just now';
      if (diffMins < 60) return `${diffMins} min${diffMins > 1 ? 's' : ''} ago`;
      if (diffHours < 24) return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`;
      if (diffDays < 7) return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`;

      return date.toLocaleDateString();
    } catch {
      return dateString;
    }
  };

  if (isLoading) {
    return (
      <div className={`p-4 ${className}`}>
        <LoadingState size="sm" message="Loading favorites..." />
      </div>
    );
  }

  if (error) {
    return (
      <div className={`p-4 ${className}`}>
        <EmptyState
          icon={<Star />}
          title="Error loading favorites"
          description={error}
          action={{
            label: 'Retry',
            onClick: loadFavorites,
          }}
        />
      </div>
    );
  }

  if (favoriteDocuments.length === 0) {
    return (
      <div className={`p-4 ${className}`}>
        <EmptyState
          icon={<Star />}
          title="No favorites yet"
          description="Star documents to quickly access them here"
        />
      </div>
    );
  }

  return (
    <div className={`flex flex-col ${className}`}>
      {/* Header */}
      <div className="px-4 py-3 border-b border-[var(--border-color)]">
        <div className="flex items-center gap-2">
          <Star className="w-5 h-5 text-[var(--warning)] fill-[var(--warning)]" />
          <h2 className="text-lg font-semibold text-[var(--text-primary)]">
            Favorites
          </h2>
          <span className="text-sm text-[var(--text-secondary)]">
            ({favoriteDocuments.length})
          </span>
        </div>
      </div>

      {/* Favorites List */}
      <div className="flex-1 overflow-y-auto">
        <div className="divide-y divide-gray-200">
          {favoriteDocuments.map((doc) => (
            <div
              key={doc.id}
              className="group px-4 py-3 hover:bg-[var(--bg-secondary)] cursor-pointer transition-colors"
              onClick={() => handleDocumentClick(doc)}
            >
              <div className="flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0">
                  {/* Document name */}
                  <div className="flex items-center gap-2 mb-1">
                    <h3 className="text-sm font-medium text-[var(--text-primary)] truncate">
                      {doc.document_name}
                    </h3>
                    <ExternalLink className="w-3.5 h-3.5 text-[var(--text-tertiary)] opacity-0 group-hover:opacity-100 transition-opacity" />
                  </div>

                  {/* File type and date */}
                  <div className="flex items-center gap-2 text-xs text-[var(--text-secondary)]">
                    {doc.file_type && (
                      <span className="uppercase font-mono">{doc.file_type}</span>
                    )}
                    {doc.file_type && <span>•</span>}
                    <span>{formatDate(doc.added_at)}</span>
                  </div>

                  {/* Path (truncated) */}
                  <div className="mt-1 text-xs text-[var(--text-tertiary)] truncate">
                    {doc.document_path}
                  </div>
                </div>

                {/* Actions */}
                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    onClick={(e) => handleShowInFolder(e, doc.document_path)}
                    className="p-1.5 rounded hover:bg-[var(--surface-hover)] transition-colors"
                    title="Show in folder"
                    aria-label="Show in folder"
                  >
                    <FolderOpen className="w-4 h-4 text-[var(--text-secondary)]" />
                  </button>
                  <button
                    onClick={(e) => handleRemoveFavorite(e, doc.document_id)}
                    className="p-1.5 rounded hover:bg-[var(--error-light)] transition-colors"
                    title="Remove from favorites"
                    aria-label="Remove from favorites"
                  >
                    <Trash2 className="w-4 h-4 text-[var(--error)]" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
});
