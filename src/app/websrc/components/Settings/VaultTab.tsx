/**
 * Vault settings — the plain-markdown mirror of your notes on disk.
 */

import { useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';

import { BackupSection } from './BackupSection';
import { GHOST_BUTTON_CLASS, SECONDARY_BUTTON_CLASS, SWITCH_CLASS } from './settingsStyles';
import {
  useSettingsQuery,
  useUpdateSettingsMutation,
} from '../../hooks/queries/useSettingsQuery';
import { toast } from '../../stores/toastStore';
import { PageHeader, SettingsRow, SettingsSection, Switch } from '../ui';

export function VaultTab() {
  const { data: settings, isLoading } = useSettingsQuery();
  const updateSettingsMutation = useUpdateSettingsMutation();
  const [isPicking, setIsPicking] = useState(false);

  const vault = settings?.vault;
  const vaultPath = vault?.vaultPath ?? '';
  const enabled = vault?.enabled ?? false;
  const watchExternal = vault?.watchExternalChanges ?? false;

  const isSyncing = isLoading;
  const isSaving = updateSettingsMutation.isPending;

  const handlePickFolder = async () => {
    setIsPicking(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Vault Folder',
      });
      if (typeof selected === 'string' && selected.trim()) {
        updateSettingsMutation.mutate(
          { category: 'vault', updates: { vaultPath: selected } },
          {
            onError: (error) => {
              toast.error("Couldn't save vault folder", { message: error.message });
            },
            onSuccess: () => {
              toast.success('Vault folder updated');
            },
          },
        );
      }
    } finally {
      setIsPicking(false);
    }
  };

  const handleResetToDefault = () => {
    updateSettingsMutation.mutate(
      { category: 'vault', updates: { vaultPath: '' } },
      {
        onError: (error) => {
          toast.error("Couldn't reset vault folder", { message: error.message });
        },
        onSuccess: () => {
          toast.success('Vault folder reset to default (~/Lattice)');
        },
      },
    );
  };

  const handleEnabledChange = (next: boolean) => {
    updateSettingsMutation.mutate(
      { category: 'vault', updates: { enabled: next } },
      {
        onError: (error) => {
          toast.error("Couldn't toggle vault export", { message: error.message });
        },
        onSuccess: () => {
          toast.success(
            next ? 'Vault export enabled — backfilling existing notes' : 'Vault export disabled',
          );
        },
      },
    );
  };

  const handleWatchChange = (next: boolean) => {
    updateSettingsMutation.mutate(
      { category: 'vault', updates: { watchExternalChanges: next } },
      {
        onError: (error) => {
          toast.error("Couldn't toggle external watcher", { message: error.message });
        },
      },
    );
  };

  return (
    <>
      <PageHeader title="Vault" />

      <SettingsSection title="Folder">
        <SettingsRow label="Path" stacked>
          <div className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-primary">
              {vaultPath || '~/Lattice'}
            </span>
            <button
              type="button"
              onClick={handlePickFolder}
              disabled={isPicking || isSaving || isSyncing}
              className={SECONDARY_BUTTON_CLASS}
            >
              Choose
            </button>
            {vaultPath ? (
              <button
                type="button"
                onClick={handleResetToDefault}
                disabled={isSaving || isSyncing}
                className={GHOST_BUTTON_CLASS}
              >
                Reset
              </button>
            ) : null}
          </div>
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title="Sync">
        <SettingsRow
          label="Mirror notes to disk"
          hint="Turning this on the first time backfills your existing notes."
        >
          <Switch
            className={SWITCH_CLASS}
            checked={enabled}
            disabled={isSaving || isSyncing}
            onCheckedChange={handleEnabledChange}
            aria-label="Mirror notes to disk"
          />
        </SettingsRow>

        <SettingsRow
          label="Watch for external edits"
          hint="Takes effect on next launch."
        >
          <Switch
            className={SWITCH_CLASS}
            checked={watchExternal}
            disabled={isSaving || isSyncing}
            onCheckedChange={handleWatchChange}
            aria-label="Watch for external edits"
          />
        </SettingsRow>
      </SettingsSection>

      <BackupSection />
    </>
  );
}
