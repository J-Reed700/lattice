/**
 * FolderList Component
 *
 * Purpose: Display and manage indexed folders
 *
 * Features:
 * - List of indexed folders with metadata
 * - Document count per folder
 * - Last sync timestamp
 * - Actions menu (remove, re-index)
 * - Empty state for first-time users
 * - Add folder button
 *
 * States: loading, empty, populated, error
 * Accessibility: WCAG AA, keyboard navigation, screen reader support
 */

import { useState, useEffect } from 'react';

import { Folder, Trash2, RefreshCw, Clock, FileText, Plus, FolderOpen } from 'lucide-react';

import VaultAPI from '../../lib/api';
import { handleAsyncEvent } from '../../utils/promiseHandlers';

import type { IndexedFolder } from '../../types';

interface FolderListProps {
  /** Callback when a folder is added */
  onFolderAdded?: (path: string) => void;
  /** Callback when a folder is removed */
  onFolderRemoved?: (path: string) => void;
  /** Callback when re-index is requested */
  onReindexRequested?: (path: string) => void;
  /** Custom CSS classes */
  className?: string;
}

export function FolderList({
  onFolderAdded,
  onFolderRemoved,
  onReindexRequested,
  className = '',
}: FolderListProps) {
  const [folders, setFolders] = useState<IndexedFolder[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [removingFolder, setRemovingFolder] = useState<string | null>(null);

  // Load indexed folders
  const loadFolders = async () => {
    setIsLoading(true);
    setError(null);

    const result = await VaultAPI.getIndexedFolders();
    if (result.ok) {
      setFolders(result.data);
    } else {
      setError(result.error);
    }

    setIsLoading(false);
  };

  useEffect(() => {
    void loadFolders();
  }, []);

  // Handle add folder
  const handleAddFolder = async () => {
    const folderPath = await VaultAPI.selectFolder();

    if (!folderPath) return;

    // Check if folder is already indexed
    if (folders.some(f => f.path === folderPath)) {
      alert('This folder is already indexed');
      return;
    }

    // Start indexing
    const result = await VaultAPI.startIndexing(folderPath);

    if (result.ok) {
      onFolderAdded?.(folderPath);
      // Reload folders to show the new one
      await loadFolders();
    } else {
      alert(`Failed to index folder: ${result.error}`);
    }
  };

  // Handle remove folder
  const handleRemoveFolder = async (path: string) => {
    setRemovingFolder(path);

    const result = await VaultAPI.removeIndexedFolder(path);

    if (result.ok) {
      setFolders(prev => prev.filter(f => f.path !== path));
      onFolderRemoved?.(path);
    } else {
      alert(`Failed to remove folder: ${result.error}`);
    }

    setRemovingFolder(null);
  };

  // Handle re-index folder
  const handleReindexFolder = async (path: string) => {
    const result = await VaultAPI.startIndexing(path);

    if (result.ok) {
      onReindexRequested?.(path);
    } else {
      alert(`Failed to re-index folder: ${result.error}`);
    }
  };

  // Format timestamp
  const formatTimestamp = (timestamp: string): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;

    return date.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined
    });
  };

  // Format folder path (show only last 2-3 segments for readability)
  const formatFolderPath = (path: string): string => {
    const parts = path.split(/[\\/]/);
    if (parts.length <= 3) return path;
    return `.../${parts.slice(-3).join('/')}`;
  };

  // Loading state
  if (isLoading) {
    return (
      <div className={`flex items-center justify-center p-8 ${className}`}>
        <div className="text-center">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-[hsl(var(--accent))] mx-auto mb-3" />
          <p className="text-sm text-[hsl(var(--text-secondary))]">Loading folders...</p>
        </div>
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className={`p-6 ${className}`}>
        <div className="bg-[hsl(var(--danger-muted))]/20 border border-[hsl(var(--danger-muted))] rounded-lg p-4">
          <p className="text-sm text-[hsl(var(--danger-fg))]">{error}</p>
          <button
            onClick={handleAsyncEvent(loadFolders)}
            className="mt-3 text-sm text-[hsl(var(--danger-fg))] hover:text-[hsl(var(--danger-fg))] font-medium"
          >
            Try again
          </button>
        </div>
      </div>
    );
  }

  // Empty state
  if (folders.length === 0) {
    return (
      <div className={`p-6 ${className}`}>
        <div className="text-center py-12">
          <div className="inline-flex items-center justify-center w-16 h-16 bg-[hsl(var(--accent-muted))]/30 rounded-full mb-4">
            <FolderOpen className="w-8 h-8 text-[hsl(var(--accent))]" />
          </div>
          <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
            No folders indexed yet
          </h3>
          <p className="text-sm text-[hsl(var(--text-secondary))] mb-6 max-w-sm mx-auto">
            Add a folder to start indexing your documents. Lattice will search through all supported files automatically.
          </p>
          <button
            onClick={handleAsyncEvent(handleAddFolder)}
            className="inline-flex items-center gap-2 px-4 py-2 bg-[hsl(var(--accent))] hover:bg-[hsl(var(--accent-hover))] text-[hsl(var(--accent-fg))] rounded-lg font-medium transition-colors duration-fast"
          >
            <Plus className="w-4 h-4" />
            Add Your First Folder
          </button>
        </div>
      </div>
    );
  }

  // Populated state
  return (
    <div className={className}>
      {/* Header with Add button */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-[hsl(var(--border-subtle))]">
        <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">
          Indexed Folders ({folders.length})
        </h3>
        <button
          onClick={handleAsyncEvent(handleAddFolder)}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-[hsl(var(--accent))] hover:bg-[hsl(var(--accent-muted))] rounded transition-colors duration-fast"
          aria-label="Add folder"
        >
          <Plus className="w-4 h-4" />
          Add Folder
        </button>
      </div>

      {/* Folders list */}
      <div className="divide-y divide-gray-200">
        {folders.map((folder) => (
          <div
            key={folder.path}
            className="group p-4 hover:bg-[hsl(var(--surface-raised))]/50 transition-colors duration-fast"
          >
            <div className="flex items-start gap-3">
              {/* Folder icon */}
              <div className="flex-shrink-0 mt-1">
                <div className="w-10 h-10 bg-[hsl(var(--accent-muted))]/30 rounded-lg flex items-center justify-center">
                  <Folder className="w-5 h-5 text-[hsl(var(--accent))]" />
                </div>
              </div>

              {/* Folder info */}
              <div className="flex-1 min-w-0">
                <div className="flex items-start justify-between gap-2">
                  <div className="flex-1 min-w-0">
                    <h4 className="text-sm font-medium text-[hsl(var(--text-primary))] truncate">
                      {formatFolderPath(folder.path)}
                    </h4>
                    <p className="text-xs text-[hsl(var(--text-tertiary))] mt-0.5 truncate font-mono">
                      {folder.path}
                    </p>
                  </div>

                  {/* Action buttons */}
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onClick={handleAsyncEvent(async () => {
                        await handleReindexFolder(folder.path);
                      })}
                      className="p-1.5 hover:bg-[hsl(var(--surface-raised))] rounded transition-colors duration-fast"
                      aria-label="Re-index folder"
                      title="Re-index folder"
                    >
                      <RefreshCw className="w-4 h-4 text-[hsl(var(--text-secondary))]" />
                    </button>
                    <button
                      onClick={handleAsyncEvent(async () => {
                        await handleRemoveFolder(folder.path);
                      })}
                      disabled={removingFolder === folder.path}
                      className="p-1.5 hover:bg-[hsl(var(--danger-muted))] rounded transition-colors duration-fast disabled:opacity-50"
                      aria-label="Remove folder"
                      title="Remove folder"
                    >
                      <Trash2 className="w-4 h-4 text-[hsl(var(--danger-fg))]" />
                    </button>
                  </div>
                </div>

                {/* Metadata */}
                <div className="flex items-center gap-4 mt-2">
                  <div className="flex items-center gap-1.5 text-xs text-[hsl(var(--text-secondary))]">
                    <FileText className="w-3.5 h-3.5" />
                    <span>
                      {folder.document_count} document{folder.document_count !== 1 ? 's' : ''}
                    </span>
                  </div>
                  <div className="flex items-center gap-1.5 text-xs text-[hsl(var(--text-secondary))]">
                    <Clock className="w-3.5 h-3.5" />
                    <span>
                      {formatTimestamp(folder.last_scan ?? '')}
                    </span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
