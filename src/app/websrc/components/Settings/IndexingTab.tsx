/**
 * Indexing Settings Tab
 *
 * Configure file watching, indexing behavior, and exclusion patterns.
 */

import { useEffect, useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';
import { Database, FolderPlus, X, Plus } from 'lucide-react';

import { VaultAPI } from '../../lib/api';
import { useSettingsStore } from '../../stores/settingsStore';
import { toast } from '../../stores/toastStore';

import type { AppConfig } from '../../types';

type IndexingConfig = AppConfig;

export function IndexingTab() {
  const indexingSettings = useSettingsStore((state) => state.settings.indexing);
  const updateIndexing = useSettingsStore((state) => state.updateIndexing);
  const addWatchFolder = useSettingsStore((state) => state.addWatchFolder);
  const removeWatchFolder = useSettingsStore((state) => state.removeWatchFolder);
  const addExcludePattern = useSettingsStore((state) => state.addExcludePattern);
  const removeExcludePattern = useSettingsStore((state) => state.removeExcludePattern);

  const [newPattern, setNewPattern] = useState('');
  const [isAddingFolder, setIsAddingFolder] = useState(false);
  const [isSyncing, setIsSyncing] = useState(true);
  const [isSavingBatchSize, setIsSavingBatchSize] = useState(false);
  const [lastSavedBatchSize, setLastSavedBatchSize] = useState(indexingSettings.batchSize);
  const [config, setConfig] = useState<IndexingConfig | null>(null);

  useEffect(() => {
    let isMounted = true;

    const loadSettings = async () => {
      setIsSyncing(true);
      const [configResult, settingsResult] = await Promise.all([
        VaultAPI.getConfig(),
        VaultAPI.getSettings(),
      ]);

      if (!isMounted) return;

      if (configResult.ok) {
        const nextConfig = configResult.data;
        setConfig(nextConfig);
        updateIndexing({
          autoIndex: nextConfig.autoIndex,
          watchFolders: nextConfig.indexedPaths,
          excludePatterns: nextConfig.excludePatterns,
        });
      } else {
        toast.error("Couldn't load indexing config", { message: configResult.error });
      }

      if (settingsResult.ok) {
        const backendBatchSize = settingsResult.data.indexing.batchSize;
        updateIndexing({ batchSize: backendBatchSize });
        setLastSavedBatchSize(backendBatchSize);
      } else {
        toast.error("Couldn't load indexing settings", { message: settingsResult.error });
      }

      setIsSyncing(false);
    };

    void loadSettings();

    return () => {
      isMounted = false;
    };
  }, [updateIndexing]);

  const persistConfig = async (nextConfig: IndexingConfig): Promise<boolean> => {
    const result = await VaultAPI.saveConfig(nextConfig);
    if (!result.ok) {
      toast.error("Couldn't save indexing config", { message: result.error });
      return false;
    }

    setConfig(nextConfig);
    return true;
  };

  const handleAutoIndexChange = async (checked: boolean) => {
    if (!config) return;

    const previous = indexingSettings.autoIndex;
    updateIndexing({ autoIndex: checked });

    const nextConfig: IndexingConfig = { ...config, autoIndex: checked };
    const configSaved = await persistConfig(nextConfig);
    if (!configSaved) {
      updateIndexing({ autoIndex: previous });
      return;
    }

    const settingsSaved = await VaultAPI.updateSettings({
      category: 'indexing',
      updates: { autoIndexNewFiles: checked },
    });

    if (!settingsSaved.ok) {
      toast.error("Couldn't sync auto-index setting", { message: settingsSaved.error });
    }
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
        if (indexingSettings.watchFolders.includes(selected)) {
          return;
        }

        addWatchFolder(selected);
        const result = await VaultAPI.addWatchFolder(selected);
        if (!result.ok) {
          removeWatchFolder(selected);
          toast.error("Couldn't add watch folder", { message: result.error });
          return;
        }

        setConfig((current) =>
          current
            ? {
                ...current,
                indexedPaths: [...current.indexedPaths, selected],
              }
            : current
        );
      }
    } catch (error) {
      console.error("Couldn't select folder:", error);
    } finally {
      setIsAddingFolder(false);
    }
  };

  const handleRemoveFolder = async (folder: string) => {
    removeWatchFolder(folder);
    const result = await VaultAPI.removeWatchFolder(folder);
    if (!result.ok) {
      addWatchFolder(folder);
      toast.error("Couldn't remove watch folder", { message: result.error });
      return;
    }

    setConfig((current) =>
      current
        ? {
            ...current,
            indexedPaths: current.indexedPaths.filter((path) => path !== folder),
          }
        : current
    );
  };

  const handleAddPattern = async () => {
    const trimmedPattern = newPattern.trim();
    if (!trimmedPattern || !config) {
      return;
    }

    if (indexingSettings.excludePatterns.includes(trimmedPattern)) {
      setNewPattern('');
      return;
    }

    const previousPatterns = indexingSettings.excludePatterns;
    const nextPatterns = [...previousPatterns, trimmedPattern];
    addExcludePattern(trimmedPattern);
    setNewPattern('');

    const nextConfig = { ...config, excludePatterns: nextPatterns };
    const saved = await persistConfig(nextConfig);
    if (!saved) {
      updateIndexing({ excludePatterns: previousPatterns });
    }
  };

  const handleRemovePattern = async (pattern: string) => {
    if (!config) return;

    const previousPatterns = indexingSettings.excludePatterns;
    const nextPatterns = previousPatterns.filter((existing) => existing !== pattern);

    removeExcludePattern(pattern);
    const nextConfig = { ...config, excludePatterns: nextPatterns };
    const saved = await persistConfig(nextConfig);
    if (!saved) {
      updateIndexing({ excludePatterns: previousPatterns });
    }
  };

  const handleBatchSizeChange = (value: number) => {
    updateIndexing({ batchSize: value });
  };

  const commitBatchSize = async () => {
    const nextBatchSize = indexingSettings.batchSize;
    if (nextBatchSize === lastSavedBatchSize) {
      return;
    }

    setIsSavingBatchSize(true);
    const result = await VaultAPI.updateSettings({
      category: 'indexing',
      updates: { batchSize: nextBatchSize },
    });
    setIsSavingBatchSize(false);

    if (!result.ok) {
      updateIndexing({ batchSize: lastSavedBatchSize });
      toast.error("Couldn't save batch size", { message: result.error });
      return;
    }

    setLastSavedBatchSize(nextBatchSize);
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
          checked={indexingSettings.autoIndex}
          onChange={(e) => {
            void handleAutoIndexChange(e.target.checked);
          }}
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
          Batch Size: {indexingSettings.batchSize}
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
          value={indexingSettings.batchSize}
          onChange={(e) => handleBatchSizeChange(parseInt(e.target.value, 10))}
          onMouseUp={() => {
            void commitBatchSize();
          }}
          onTouchEnd={() => {
            void commitBatchSize();
          }}
          onKeyUp={() => {
            void commitBatchSize();
          }}
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

        {indexingSettings.watchFolders.length === 0 ? (
          <div className="p-8 text-center bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
            <FolderPlus className="w-12 h-12 text-[hsl(var(--text-tertiary))] mx-auto mb-3" />
            <p className="text-sm text-[hsl(var(--text-secondary))]">
              No folders are being watched
            </p>
            <p className="text-xs text-[hsl(var(--text-tertiary))] mt-1">
              Click "Add Folder" to start indexing files
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {indexingSettings.watchFolders.map((folder, index) => (
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
                handleAddPattern();
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
          {indexingSettings.excludePatterns.map((pattern, index) => (
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

        {indexingSettings.excludePatterns.length === 0 && (
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
