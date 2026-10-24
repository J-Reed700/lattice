import { useEffect, useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';
import { X } from 'lucide-react';

import { PageHeader, SettingsRow, SettingsSection, settingsFieldClass, Switch } from '@/components/ui';
import { IndexingActivitySection } from '@/features/files/components/IndexingStatus/IndexingActivitySection';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';

import { NUMBER_FIELD_CLASS, ROW_ACTION_CLASS, SECONDARY_BUTTON_CLASS, SWITCH_CLASS } from './settingsStyles';
import {
  useSettingsQuery,
  useUpdateSettingsMutation,
  useWatchFolderMutations,
} from '../hooks/useSettingsQuery';

const DEFAULT_BATCH_SIZE = 32;

export function IndexingTab() {
  const { data: settings, isLoading } = useSettingsQuery();
  const updateSettingsMutation = useUpdateSettingsMutation();
  const { addWatchFolder, removeWatchFolder } = useWatchFolderMutations();

  const [newPattern, setNewPattern] = useState('');
  const [isAddingFolder, setIsAddingFolder] = useState(false);

  // Batch size keeps a draft so we commit once on blur/Enter, not per keystroke.
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

        const result = await addWatchFolder(selected);
        if (!result.ok) {
          toast.error("Couldn't add watch folder", { message: result.error });
        }
      }
    } catch (error) {
      console.error("Couldn't select folder:", error);
    } finally {
      setIsAddingFolder(false);
    }
  };

  const handleRemoveFolder = async (folder: string) => {
    const result = await removeWatchFolder(folder);
    if (!result.ok) {
      toast.error("Couldn't remove watch folder", { message: result.error });
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
          setBatchSizeDraft(backendBatchSize); // roll back the draft
          toast.error("Couldn't save batch size", { message: error.message });
        },
      }
    );
  };

  return (
    <>
      <PageHeader title="Indexing" description="Choose what enters your library and how it stays up to date." />

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
          <input
            id="batchSize"
            type="number"
            min={1}
            max={128}
            step={1}
            value={batchSizeDraft}
            onChange={(e) => {
              const next = parseInt(e.target.value, 10);
              if (!Number.isNaN(next)) setBatchSizeDraft(Math.min(128, Math.max(1, next)));
            }}
            onBlur={commitBatchSize}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commitBatchSize();
            }}
            disabled={isSyncing || isSavingBatchSize}
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection
        title="Watched folders"
        actions={
          <button
            type="button"
            onClick={handleAddFolder}
            disabled={isAddingFolder || isSyncing}
            className={SECONDARY_BUTTON_CLASS}
          >
            {isAddingFolder ? 'Adding…' : 'Add folder'}
          </button>
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
            <input
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
              className={cn(settingsFieldClass, 'flex-1')}
            />
            <button
              type="button"
              onClick={() => {
                void handleAddPattern();
              }}
              disabled={!newPattern.trim() || isSyncing}
              className={SECONDARY_BUTTON_CLASS}
            >
              Add
            </button>
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
