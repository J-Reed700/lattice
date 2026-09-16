/**
 * BackupSection
 *
 * Back up, restore, and export — the three things a user does when they want
 * their data out of, or back into, Lattice.
 *
 * There is no folder picker. `BackupAdapter::create_backup` confines writes to
 * the app's backups folder and returns PermissionDenied for anything else, so a
 * native dialog here would produce a security error the user could not act on.
 * Both backup and export write to the app's own folders and hand the user a
 * "Show" action instead.
 */

import { useMemo, useState } from 'react';

import { format } from 'date-fns';
import { Archive, Download } from 'lucide-react';

import type { BackupInfo } from '@/types/api/backup';

import { GHOST_BUTTON_CLASS, SECONDARY_BUTTON_CLASS } from './settingsStyles';
import {
  useBackupsQuery,
  useCreateBackupMutation,
  useExportMutation,
  useRestoreBackupMutation,
  type ExportFormat,
} from '../../hooks/queries/useBackupsQuery';
import {
  isIndexingActive,
  useIndexingStatusQuery,
} from '../../hooks/queries/useIndexingStatusQuery';
import { useRegisterPaletteCommands } from '../../hooks/useRegisterPaletteCommands';
import VaultAPI from '../../lib/api';
import { toast } from '../../stores/toastStore';
import { ConfirmDialog } from '../ConfirmDialog/ConfirmDialog';
import { SettingsRow, SettingsSection } from '../ui';

import type { PaletteCommand } from '../../stores/paletteCommandsStore';

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 && unit > 0 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

function formatBackupDate(createdAt: string): string | null {
  if (!createdAt) return null;
  const date = new Date(createdAt);
  if (Number.isNaN(date.getTime())) return null;
  return format(date, "MMM d, yyyy 'at' HH:mm");
}

export function BackupSection() {
  const { data: backups } = useBackupsQuery();
  const createBackup = useCreateBackupMutation();
  const restoreBackup = useRestoreBackupMutation();
  const exportData = useExportMutation();
  const { data: indexing } = useIndexingStatusQuery();

  const [pendingRestore, setPendingRestore] = useState<BackupInfo | null>(null);
  const [restored, setRestored] = useState(false);

  const isIndexing = isIndexingActive(indexing);
  const isBackingUp = createBackup.isPending;
  const isExporting = exportData.isPending;

  const runBackup = useMemo(
    () => () => {
      createBackup.mutate(undefined, {
        onSuccess: (result) => {
          toast.success('Backup created', {
            message: 'Saved to your backups folder.',
            action: {
              label: 'Show',
              onClick: () => void VaultAPI.showInFolder(result.backupPath),
            },
          });
        },
        onError: (error) => {
          toast.error("Couldn't back up", { message: error.message });
        },
      });
    },
    [createBackup],
  );

  const runExport = useMemo(
    () => (exportFormat: ExportFormat) => {
      exportData.mutate(exportFormat, {
        onSuccess: (summary) => {
          if (summary.count === 0) {
            toast.info('Nothing to export yet.');
            return;
          }
          toast.success(
            `Exported ${summary.count} ${summary.count === 1 ? 'item' : 'items'}`,
            {
              message:
                exportFormat === 'markdown'
                  ? 'Markdown files are in your exports folder.'
                  : 'The JSON file is in your exports folder.',
              action: {
                label: 'Show',
                onClick: () => void VaultAPI.showInFolder(summary.outputDir),
              },
            },
          );
        },
        onError: (error) => {
          toast.error("Couldn't export", { message: error.message });
        },
      });
    },
    [exportData],
  );

  const paletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'vault.backup',
        label: 'Back up now',
        group: 'Vault',
        icon: Archive,
        enabled: !isBackingUp && !restored,
        run: runBackup,
      },
      {
        id: 'vault.export.markdown',
        label: 'Export conversations and journals',
        group: 'Vault',
        icon: Download,
        enabled: !isExporting && !restored,
        run: () => runExport('markdown'),
      },
    ],
    [isBackingUp, isExporting, restored, runBackup, runExport],
  );
  useRegisterPaletteCommands(paletteCommands);

  const confirmRestore = () => {
    if (!pendingRestore) return;
    const target = pendingRestore;
    restoreBackup.mutate(target.path, {
      onSuccess: () => {
        setPendingRestore(null);
        setRestored(true);
        toast.success('Backup restored', {
          message: 'Quit and reopen Lattice.',
          duration: 10000,
        });
      },
      onError: (error) => {
        setPendingRestore(null);
        toast.error("Couldn't restore that backup", { message: error.message });
      },
    });
  };

  if (restored) {
    return (
      <SettingsSection title="Backups">
        <div className="border-b border-border-subtle py-3">
          <p className="text-sm text-text-primary">Backup restored.</p>
          <p className="mt-0.5 text-sm text-text-secondary">
            Quit Lattice and open it again to use the restored library.
          </p>
        </div>
      </SettingsSection>
    );
  }

  const restoreDate = pendingRestore
    ? (formatBackupDate(pendingRestore.createdAt) ?? pendingRestore.name)
    : '';

  return (
    <>
      <SettingsSection
        title="Backups"
        actions={
          <button
            type="button"
            onClick={runBackup}
            disabled={isBackingUp || isIndexing}
            className={SECONDARY_BUTTON_CLASS}
          >
            {isBackingUp ? 'Backing up…' : 'Back up now'}
          </button>
        }
      >
        {!backups || backups.length === 0 ? (
          <div className="border-b border-border-subtle py-3 text-sm text-text-muted">
            No backups yet.
          </div>
        ) : (
          backups.map((backup) => {
            const date = formatBackupDate(backup.createdAt);
            const size = formatBytes(backup.size);
            const meta = [
              backup.fileCount > 0
                ? `${backup.fileCount.toLocaleString()} documents`
                : null,
              size || null,
            ]
              .filter(Boolean)
              .join(' · ');

            return (
              <div
                key={backup.path}
                className="flex items-center justify-between gap-4 border-b border-border-subtle py-2.5"
              >
                <div className="flex min-w-0 flex-1 items-baseline gap-3">
                  {date ? (
                    <span className="truncate text-sm text-text-primary">{date}</span>
                  ) : (
                    <span className="truncate font-mono text-xs text-text-primary">
                      {backup.name}
                    </span>
                  )}
                  {meta ? (
                    <span className="shrink-0 text-xs tabular-nums text-text-muted">{meta}</span>
                  ) : null}
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <button
                    type="button"
                    onClick={() => setPendingRestore(backup)}
                    disabled={isIndexing || restoreBackup.isPending}
                    title={
                      isIndexing ? 'Wait for indexing to finish before restoring.' : undefined
                    }
                    className={GHOST_BUTTON_CLASS}
                  >
                    Restore…
                  </button>
                  <button
                    type="button"
                    onClick={() => void VaultAPI.showInFolder(backup.path)}
                    className={GHOST_BUTTON_CLASS}
                  >
                    Show
                  </button>
                </div>
              </div>
            );
          })
        )}
      </SettingsSection>

      <SettingsSection title="Export">
        <SettingsRow
          label="Conversations and journals"
          hint="Written to the app's exports folder."
        >
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => runExport('markdown')}
              disabled={isExporting}
              className={SECONDARY_BUTTON_CLASS}
            >
              Markdown
            </button>
            <button
              type="button"
              onClick={() => runExport('json')}
              disabled={isExporting}
              className={SECONDARY_BUTTON_CLASS}
            >
              JSON
            </button>
          </div>
        </SettingsRow>
      </SettingsSection>

      <ConfirmDialog
        isOpen={pendingRestore !== null}
        title="Restore this backup?"
        message={`This replaces your current library with the backup from ${restoreDate}.`}
        details="Everything indexed since then is lost. Lattice has to be quit and reopened afterwards — it can't reconnect to the restored database on its own."
        variant="danger"
        confirmLabel="Restore"
        requireConfirmation="restore"
        onConfirm={confirmRestore}
        onCancel={() => setPendingRestore(null)}
      />
    </>
  );
}
