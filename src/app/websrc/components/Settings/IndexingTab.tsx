/**
 * Indexing Settings Tab
 *
 * Configure file watching, indexing behavior, and exclusion patterns.
 *
 * Backend is the SSOT (Rust SettingsRepository). React Query is the
 * read-only mirror. Per the post-Phase-4b/Task-7 cleanup, this tab
 * uses only `useSettingsQuery` — the previous duality with AppConfig
 * is gone.
 *
 * Watch folder add/remove still go through dedicated Tauri commands
 * (`VaultAPI.addWatchFolder` / `removeWatchFolder`) so the backend can
 * run path validation (CWE-22 / CWE-158) and serialize concurrent
 * mutations. Both commands invalidate the SETTINGS query cache on
 * success — React Query refetches the canonical state.
 */

import { useEffect, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { Database, FolderPlus, X, Plus } from 'lucide-react';

import {
  SETTINGS_QUERY_KEY,
  useSettingsQuery,
  useUpdateSettingsMutation,
} from '../../hooks/queries/useSettingsQuery';
import VaultAPI from '../../lib/api';
import { toast } from '../../stores/toastStore';

const DEFAULT_BATCH_SIZE = 32;

export function IndexingTab() {
  const { data: settings, isLoading } = useSettingsQuery();
  const updateSettingsMutation = useUpdateSettingsMutation();
  const queryClient = useQueryClient();

  const [newPattern, setNewPattern] = useState('');
  const [isAddingFolder, setIsAddingFolder] = useState(false);

  // Batch size has its own draft state because it's a slider — we don't
  // want to fire a mutation on every drag tick.
  const backendBatchSize = settings?.indexing.batchSize ?? DEFAULT_BATCH_SIZE;
  const [batchSizeDraft, setBatchSizeDraft] = useState(backendBatchSize);

  useEffect(() => {
    setBatchSizeDraft(backendBatchSize);
  }, [backendBatchSize]);

  const isSyncing = isLoading;
  const isSavingBatchSize = updateSettingsMutation.isPending;

  const watchFolders = settings?.indexing.indexedPaths ?? [];
  const excludePatterns = settings?.indexing.excludePatterns ?? [];
  const autoIndex = settings?.indexing.autoIndexNewFiles ?? true;

  const invalidateSettings = () => {
    void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
  };

  const handleAutoIndexChange = (checked: boolean) => {
    updateSettingsMutation.mutate(
      { category: 'indexing', updates: { autoIndexNewFiles: checked } },
      {
        onError: (error) => {
          toast.error("Couldn't save auto-index setting", { message: error.message });
        },
      }
    );
  };

  const handleAddFolder = async () => {
    setIsAddingFolder(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder to Watch',
      });

      if (selected && typeof selected === 'string') {
        if (watchFolders.includes(selected)) return;

        const result = await VaultAPI.addWatchFolder(selected);
        if (!result.ok) {
          toast.error("Couldn't add watch folder", { message: result.error });
          return;
        }
        invalidateSettings();
      }
    } catch (error) {
      console.error("Couldn't select folder:", error);
    } finally {
      setIsAddingFolder(false);
    }
  };

  const handleRemoveFolder = async (folder: string) => {
    const result = await VaultAPI.removeWatchFolder(folder);
    if (!result.ok) {
      toast.error("Couldn't remove watch folder", { message: result.error });
      return;
    }
    invalidateSettings();
  };

  const handleAddPattern = async () => {
    const trimmedPattern = newPattern.trim();
    if (!trimmedPattern) return;

    if (excludePatterns.includes(trimmedPattern)) {
      setNewPattern('');
      return;
    }

    const next = [...excludePatterns, trimmedPattern];
    setNewPattern('');
    updateSettingsMutation.mutate(
      { category: 'indexing', updates: { excludePatterns: next } },
      {
        onError: (error) => {
          toast.error("Couldn't add exclude pattern", { message: error.message });
        },
      }
    );
  };

  const handleRemovePattern = async (pattern: string) => {
    const next = excludePatterns.filter((existing) => existing !== pattern);
    updateSettingsMutation.mutate(
      { category: 'indexing', updates: { excludePatterns: next } },
      {
        onError: (error) => {
          toast.error("Couldn't remove exclude pattern", { message: error.message });
        },
      }
    );
  };

  const commitBatchSize = () => {
    if (batchSizeDraft === backendBatchSize) return;

    updateSettingsMutation.mutate(
      { category: 'indexing', updates: { batchSize: batchSizeDraft } },
      {
        onError: (error) => {
          setBatchSizeDraft(backendBatchSize); // rollback the slider
          toast.error("Couldn't save batch size", { message: error.message });
        },
      }
    );
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[hsl(var(--border-subtle))]">
        <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
          <Database className="w-5 h-5 text-[hsl(var(--accent))]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Indexing Settings</h2>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Manage file watching and indexing behavior
          </p>
          {isSyncing ? (
            <p className="text-xs text-[hsl(var(--text-tertiary))] mt-1">Syncing with backend settings...</p>
          ) : null}
        </div>
      </div>

      {/* Auto Index */}
      <div className="flex items-start gap-3 p-4 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
        <input
          id="autoIndex"
          type="checkbox"
          checked={autoIndex}
          onChange={(e) => handleAutoIndexChange(e.target.checked)}
          disabled={isSyncing}
          className="mt-1 w-4 h-4 text-[hsl(var(--accent))] bg-[hsl(var(--surface))] border-[hsl(var(--border-subtle))] rounded focus:ring-2 focus:ring-[hsl(var(--accent))]"
        />
        <div className="flex-1">
          <label htmlFor="autoIndex" className="block text-sm font-medium text-[hsl(var(--text-primary))] cursor-pointer">
            Auto-index New Files
          </label>
          <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
            Automatically index new files added to watched folders
          </p>
        </div>
      </div>

      {/* Batch Size */}
      <div className="space-y-2">
        <label htmlFor="batchSize" className="block text-sm font-medium text-[hsl(var(--text-primary))]">
          Batch Size: {batchSizeDraft}
        </label>
        <p className="text-xs text-[hsl(var(--text-secondary))] mb-3">
          Number of files to process simultaneously. Higher values are faster but use more memory.
        </p>
        <input
          id="batchSize"
          type="range"
          min="1"
          max="128"
          step="1"
          value={batchSizeDraft}
          onChange={(e) => setBatchSizeDraft(parseInt(e.target.value, 10))}
          onMouseUp={commitBatchSize}
          onTouchEnd={commitBatchSize}
          onKeyUp={commitBatchSize}
          disabled={isSyncing || isSavingBatchSize}
          className="w-full h-2 bg-[hsl(var(--surface))] rounded-lg appearance-none cursor-pointer accent-[hsl(var(--accent))]"
        />
        <div className="flex justify-between text-xs text-[hsl(var(--text-tertiary))]">
          <span>1 (Slow)</span>
          <span>128 (Fast)</span>
        </div>
        {isSavingBatchSize ? (
          <p className="text-xs text-[hsl(var(--text-tertiary))]">Saving batch size...</p>
        ) : null}
      </div>

      {/* Watch Folders */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Watch Folders</h3>
            <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
              Folders that are automatically indexed
            </p>
          </div>
          <button
            onClick={handleAddFolder}
            disabled={isAddingFolder || isSyncing}
            className="flex items-center gap-2 px-3 py-2 text-sm bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-md hover:bg-[hsl(var(--accent-hover))] disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            <FolderPlus className="w-4 h-4" />
            {isAddingFolder ? 'Adding...' : 'Add Folder'}
          </button>
        </div>

        {watchFolders.length === 0 ? (
          <div className="p-8 text-center bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
            <FolderPlus className="w-12 h-12 text-[hsl(var(--text-tertiary))] mx-auto mb-3" />
            <p className="text-sm text-[hsl(var(--text-secondary))]">
              No folders are being watched
            </p>
            <p className="text-xs text-[hsl(var(--text-tertiary))] mt-1">
              Click &quot;Add Folder&quot; to start indexing files
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {watchFolders.map((folder, index) => (
              <div
                key={index}
                className="flex items-center justify-between p-3 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] hover:border-[hsl(var(--border-default))] transition-colors"
              >
                <div className="flex-1 min-w-0">
                  <p className="text-sm text-[hsl(var(--text-primary))] truncate font-mono">
                    {folder}
                  </p>
                </div>
                <button
                  onClick={() => {
                    void handleRemoveFolder(folder);
                  }}
                  disabled={isSyncing}
                  className="ml-3 p-1.5 text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))] rounded transition-colors"
                  title="Remove folder"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Exclude Patterns */}
      <div className="space-y-3">
        <div>
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Exclude Patterns</h3>
          <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
            File patterns to ignore during indexing (supports wildcards)
          </p>
        </div>

        {/* Add Pattern Input */}
        <div className="flex gap-2">
          <input
            type="text"
            value={newPattern}
            onChange={(e) => setNewPattern(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                void handleAddPattern();
              }
            }}
            placeholder="*.tmp, node_modules, .git"
            className="flex-1 px-3 py-2 text-sm bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] border border-[hsl(var(--border-subtle))] rounded-lg focus:outline-none focus:ring-2 focus:ring-[hsl(var(--accent))] focus:border-transparent"
          />
          <button
            onClick={() => {
              void handleAddPattern();
            }}
            disabled={!newPattern.trim() || isSyncing}
            className="px-4 py-2 text-sm bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-md hover:bg-[hsl(var(--accent-hover))] disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
          >
            <Plus className="w-4 h-4" />
            Add
          </button>
        </div>

        {/* Pattern List */}
        <div className="flex flex-wrap gap-2">
          {excludePatterns.map((pattern, index) => (
            <div
              key={index}
              className="flex items-center gap-2 px-3 py-1.5 bg-[hsl(var(--surface-raised))] rounded-full text-sm"
            >
              <code className="text-[hsl(var(--text-primary))]">{pattern}</code>
              <button
                onClick={() => {
                  void handleRemovePattern(pattern);
                }}
                disabled={isSyncing}
                className="text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--danger-fg))] transition-colors"
                title="Remove pattern"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
          ))}
        </div>

        {excludePatterns.length === 0 && (
          <div className="p-4 text-center bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
            <p className="text-sm text-[hsl(var(--text-secondary))]">
              No exclusion patterns defined
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
