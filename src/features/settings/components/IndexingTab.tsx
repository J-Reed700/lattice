/**
 * Indexing settings.
 *
 * Backend is the SSOT (Rust SettingsRepository). React Query is the
 * read-only mirror. Per the post-Phase-4b/Task-7 cleanup, this tab
 * uses only `useSettingsQuery` — the previous duality with AppConfig
 * is gone.
 *
 * Watch folder add/remove still go through dedicated Tauri commands
 * (`useIndexedFoldersActions`) so the backend can run path validation
 * (CWE-22 / CWE-158) and serialize concurrent mutations. Both invalidate
 * the SETTINGS query cache on success — React Query refetches the
 * canonical state.
 */

import { useEffect, useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';
import { X } from 'lucide-react';

import { Button, Input, PageHeader, SettingsRow, SettingsSection, Switch } from '@/components/ui';
import { IndexingActivitySection } from '@/features/files/components/IndexingStatus/IndexingActivitySection';
import { useIndexedFoldersActions } from '@/features/files/hooks/useIndexedFoldersQuery';
import { getErrorMessage } from '@/lib/errorUtils';
import { toast } from '@/stores/toastStore';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { SettingsNumberField } from './SettingControls';
import { ROW_ACTION_CLASS, SWITCH_CLASS } from './settingsStyles';
import { useSettingsQuery, useUpdateSettingsMutation } from '../hooks/useSettingsQuery';

const DEFAULT_BATCH_SIZE = 32;

export function IndexingTab() {
  const { data: settings, isLoading } = useSettingsQuery();
  const updateSettingsMutation = useUpdateSettingsMutation();
  const { addFolder, removeFolder } = useIndexedFoldersActions();

  const [newPattern, setNewPattern] = useState('');
  const [isAddingFolder, setIsAddingFolder] = useState(false);

  // Batch size keeps a draft so we commit once on blur/Enter/step, not per keystroke.
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

  const handleAutoIndexChange = (checked: boolean) => {
    updateSettingsMutation.mutate(
      { category: 'indexing', updates: { autoIndexNewFiles: checked } },
      {
        onError: (error) => {
          toast.error("Couldn't save auto-index setting", { message: error.message });
        },
      },
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

        try {
          await addFolder(selected);
        } catch (error) {
          toast.error("Couldn't add watch folder", { message: getErrorMessage(error) });
        }
      }
    } catch (error) {
      console.error("Couldn't select folder:", error);
    } finally {
      setIsAddingFolder(false);
    }
  };

  const handleRemoveFolder = async (folder: string) => {
    try {
      await removeFolder(folder);
    } catch (error) {
      toast.error("Couldn't remove watch folder", { message: getErrorMessage(error) });
    }
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
      },
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
      },
    );
  };

  const commitBatchSize = (candidate: number) => {
    const next = Math.max(1, Math.round(candidate));
    if (next === backendBatchSize) return;

    updateSettingsMutation.mutate(
      { category: 'indexing', updates: { batchSize: next } },
      {
        onError: (error) => {
          setBatchSizeDraft(backendBatchSize); // roll back the draft
          toast.error("Couldn't save batch size", { message: error.message });
        },
      },
    );
  };

  return (
    <>
      <PageHeader
        title="Indexing"
        description="Choose what enters your library and how it stays up to date."
      />

      <SettingsSection title="Behavior">
        <SettingsRow label="Auto-index new files" htmlFor="autoIndex">
          <Switch
            id="autoIndex"
            checked={autoIndex}
            onCheckedChange={(checked) => handleAutoIndexChange(checked)}
            disabled={isSyncing}
            className={SWITCH_CLASS}
          />
        </SettingsRow>

        <SettingsRow
          label="Batch size"
          hint={isSavingBatchSize ? 'Files indexed per pass · saving…' : 'Files indexed per pass'}
          htmlFor="batchSize"
        >
          <SettingsNumberField
            id="batchSize"
            min={1}
            step={1}
            integer
            value={batchSizeDraft}
            disabled={isSyncing || isSavingBatchSize}
            onDraftChange={(next) => setBatchSizeDraft(Math.max(1, Math.round(next)))}
            onCommit={commitBatchSize}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection
        title="Watched folders"
        actions={
          <Button
            type="button"
            variant="outline"
            onClick={handleAsyncEvent(handleAddFolder)}
            disabled={isAddingFolder || isSyncing}
          >
            {isAddingFolder ? 'Adding…' : 'Add folder'}
          </Button>
        }
      >
        {watchFolders.length === 0 ? (
          <div className="border-b border-border-subtle py-3 text-sm text-text-muted">
            No folders watched. Add one to start indexing.
          </div>
        ) : (
          watchFolders.map((folder) => (
            <div
              key={folder}
              className="flex items-center justify-between gap-4 border-b border-border-subtle py-2.5"
            >
              <span className="truncate font-mono text-xs text-text-primary">{folder}</span>
              <button
                type="button"
                onClick={() => {
                  void handleRemoveFolder(folder);
                }}
                disabled={isSyncing}
                className={ROW_ACTION_CLASS}
                title="Remove folder"
                aria-label={`Remove folder ${folder}`}
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          ))
        )}
      </SettingsSection>

      <SettingsSection title="Excluded patterns">
        <SettingsRow label="Add a pattern" stacked htmlFor="excludePattern">
          <div className="flex gap-2">
            <div className="min-w-0 flex-1">
              <Input
                id="excludePattern"
                type="text"
                value={newPattern}
                onChange={(e) => setNewPattern(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    void handleAddPattern();
                  }
                }}
                placeholder="*.tmp, node_modules, .git"
              />
            </div>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                void handleAddPattern();
              }}
              disabled={!newPattern.trim() || isSyncing}
            >
              Add
            </Button>
          </div>
        </SettingsRow>

        {excludePatterns.length === 0 ? (
          <div className="border-b border-border-subtle py-3 text-sm text-text-muted">
            Nothing excluded.
          </div>
        ) : (
          excludePatterns.map((pattern) => (
            <div
              key={pattern}
              className="flex items-center justify-between gap-4 border-b border-border-subtle py-2.5"
            >
              <span className="truncate font-mono text-xs text-text-primary">{pattern}</span>
              <button
                type="button"
                onClick={() => {
                  void handleRemovePattern(pattern);
                }}
                disabled={isSyncing}
                className={ROW_ACTION_CLASS}
                title="Remove pattern"
                aria-label={`Remove pattern ${pattern}`}
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          ))
        )}
      </SettingsSection>

      <IndexingActivitySection />
    </>
  );
}
