/**
 * BackupSection
 *
 * Back up, restore, and export — the three things a user does when they want
 * their data out of, or back into, Lattice.
 *
 * There is no folder picker for the local backups. `BackupAdapter::create_backup`
 * confines writes to the app's backups folder and returns PermissionDenied for
 * anything else, so a native dialog here would produce a security error the
 * user could not act on. Both backup and export write to the app's own folders
 * and hand the user a "Show" action instead.
 *
 * Off-device backup is the other half: one encrypted file per run, written to a
 * folder the user picks. That folder is chosen by a native dialog on the Rust
 * side and never crosses IPC, which is why every archive command here takes no
 * path argument.
 */

import { useMemo, useState } from 'react';

import { format } from 'date-fns';
import { Archive, Download, Upload } from 'lucide-react';

import type { ArchiveStatus, BackupInfo } from '@/types/api/backup';

import { ArchiveSetupWizard, type ArchiveWizardMode } from './ArchiveSetupWizard';
import {
  GHOST_BUTTON_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from './settingsStyles';
import {
  useArchiveStatusQuery,
  useCreateArchiveNowMutation,
  useDisableArchiveMutation,
  useChooseArchiveDestinationMutation,
  useRestoreArchiveMutation,
  useSetArchiveKeepCountMutation,
  useSetArchivePassphraseMutation,
} from '../../hooks/queries/useArchiveQuery';
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
import { SettingsRow, SettingsSection, settingsFieldClass } from '../ui';
import { Badge } from '../ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';

import type { PaletteCommand } from '../../stores/paletteCommandsStore';

const KEEP_COUNT_OPTIONS = [1, 3, 5, 10, 20];
const MIN_PASSPHRASE_LENGTH = 8;

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

/**
 * `message` on a `not_hydrated` result is the provider's name, so the sentence
 * is built here rather than shown raw.
 */
function hydrationMessage(provider: string | null): string {
  return provider
    ? `That archive is still only in ${provider} — it hasn't been downloaded to this computer yet. Open the folder to download it, then try again.`
    : "That archive hasn't been downloaded to this computer yet. Download it in your cloud folder, then try again.";
}

export function BackupSection() {
  const { data: backups } = useBackupsQuery();
  const createBackup = useCreateBackupMutation();
  const restoreBackup = useRestoreBackupMutation();
  const exportData = useExportMutation();
  const { data: indexing } = useIndexingStatusQuery();

  const { data: archive } = useArchiveStatusQuery();
  const createArchive = useCreateArchiveNowMutation();
  const restoreArchive = useRestoreArchiveMutation();
  const chooseDestination = useChooseArchiveDestinationMutation();
  const setKeepCount = useSetArchiveKeepCountMutation();
  const setPassphrase = useSetArchivePassphraseMutation();
  const disableArchive = useDisableArchiveMutation();

  const [pendingRestore, setPendingRestore] = useState<BackupInfo | null>(null);
  const [restored, setRestored] = useState(false);
  const [restoredFromArchive, setRestoredFromArchive] = useState(false);

  const [wizardMode, setWizardMode] = useState<ArchiveWizardMode | null>(null);
  const [secretPrompt, setSecretPrompt] = useState(false);
  const [secret, setSecret] = useState('');
  const [secretError, setSecretError] = useState<string | null>(null);
  const [hydrationNotice, setHydrationNotice] = useState<string | null>(null);
  const [lastRestoreSecret, setLastRestoreSecret] = useState<string | null>(null);
  const [passphraseDialogOpen, setPassphraseDialogOpen] = useState(false);
  const [newPassphrase, setNewPassphrase] = useState('');
  const [newPassphraseAgain, setNewPassphraseAgain] = useState('');
  const [passphraseError, setPassphraseError] = useState<string | null>(null);
  const [confirmingTurnOff, setConfirmingTurnOff] = useState(false);

  const isIndexing = isIndexingActive(indexing);
  const isBackingUp = createBackup.isPending;
  const isExporting = exportData.isPending;
  const isArchiving = createArchive.isPending;
  const isRestoringArchive = restoreArchive.isPending;
  const archiveConfigured = archive?.configured === true;
  const archiveReady = archiveConfigured && Boolean(archive?.destination);

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

  const runArchiveNow = useMemo(
    () => () => {
      createArchive.mutate(undefined, {
        onSuccess: (run) => {
          const size = formatBytes(run.size);
          toast.success('Backed up to your folder', {
            message: size ? `One encrypted file, ${size}.` : 'One encrypted file.',
          });
        },
        onError: (error) => {
          toast.error("Couldn't back up to your folder", { message: error.message });
        },
      });
    },
    [createArchive],
  );

  /**
   * One entry point for every restore attempt. `secret` is null on the first
   * try — the key is usually in the keyring already — and carries the
   * passphrase or the recovery code on the retry the backend asked for.
   */
  const runArchiveRestore = useMemo(
    () => (attemptSecret: string | null) => {
      setLastRestoreSecret(attemptSecret);
      setHydrationNotice(null);
      restoreArchive.mutate(attemptSecret, {
        onSuccess: (result) => {
          switch (result.outcome) {
            case 'restored':
              setSecretPrompt(false);
              setSecret('');
              setSecretError(null);
              setRestored(true);
              setRestoredFromArchive(true);
              toast.success('Backup restored', {
                message: 'Quit and reopen Lattice.',
                duration: 10000,
              });
              break;
            case 'needs_secret':
              setSecretError(null);
              setSecretPrompt(true);
              break;
            case 'not_hydrated':
              setSecretPrompt(false);
              setHydrationNotice(hydrationMessage(result.message));
              break;
            case 'cancelled':
            default:
              break;
          }
        },
        onError: (error) => {
          // A wrong passphrase or recovery code lands here; the dialog stays
          // open so the next attempt does not start from the file picker.
          if (attemptSecret !== null) {
            setSecretError(error.message);
            setSecretPrompt(true);
            return;
          }
          toast.error("Couldn't restore that backup", { message: error.message });
        },
      });
    },
    [restoreArchive],
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
      {
        id: 'vault.archive.backup',
        label: 'Back up to cloud folder now',
        group: 'Vault',
        icon: Upload,
        enabled: archiveReady && !isArchiving && !isIndexing && !restored,
        run: runArchiveNow,
      },
      {
        id: 'vault.archive.restore',
        label: 'Restore from backup file',
        group: 'Vault',
        icon: Download,
        enabled: archiveConfigured && !isRestoringArchive && !restored,
        run: () => runArchiveRestore(null),
      },
    ],
    [
      archiveConfigured,
      archiveReady,
      isArchiving,
      isBackingUp,
      isExporting,
      isIndexing,
      isRestoringArchive,
      restored,
      runArchiveNow,
      runArchiveRestore,
      runBackup,
      runExport,
    ],
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

  const submitPassphrase = () => {
    if (newPassphrase.length < MIN_PASSPHRASE_LENGTH) {
      setPassphraseError(
        `The passphrase has to be at least ${MIN_PASSPHRASE_LENGTH} characters.`,
      );
      return;
    }
    if (newPassphrase !== newPassphraseAgain) {
      setPassphraseError("The two passphrases don't match.");
      return;
    }
    setPassphraseError(null);
    setPassphrase.mutate(newPassphrase, {
      onSuccess: () => {
        setPassphraseDialogOpen(false);
        setNewPassphrase('');
        setNewPassphraseAgain('');
        toast.success('Passphrase saved');
      },
      onError: (error) => setPassphraseError(error.message),
    });
  };

  const removePassphrase = () => {
    setPassphraseError(null);
    setPassphrase.mutate(null, {
      onSuccess: () => {
        setPassphraseDialogOpen(false);
        setNewPassphrase('');
        setNewPassphraseAgain('');
        toast.success('Passphrase removed', {
          message: 'The recovery code is now the only way into your archives.',
        });
      },
      onError: (error) => setPassphraseError(error.message),
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
          {restoredFromArchive ? (
            <p className="mt-0.5 text-sm text-text-secondary">
              Search indexes rebuild on the next launch, so search will be incomplete until that
              finishes.
            </p>
          ) : null}
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

      {archive ? (
        <OffDeviceBackup
          archive={archive}
          isIndexing={isIndexing}
          isArchiving={isArchiving}
          isRestoring={isRestoringArchive}
          isChangingDestination={chooseDestination.isPending}
          isSavingKeepCount={setKeepCount.isPending}
          hydrationNotice={hydrationNotice}
          onSetUp={() => setWizardMode('setup')}
          onChangeDestination={() => {
            chooseDestination.mutate(undefined, {
              onError: (error) =>
                toast.error("Couldn't change the folder", { message: error.message }),
            });
          }}
          onKeepCountChange={(value) => {
            setKeepCount.mutate(value, {
              onError: (error) =>
                toast.error("Couldn't change how many to keep", { message: error.message }),
            });
          }}
          onBackUpNow={runArchiveNow}
          onRestore={() => runArchiveRestore(null)}
          onRetryHydration={() => runArchiveRestore(lastRestoreSecret)}
          onManagePassphrase={() => {
            setNewPassphrase('');
            setNewPassphraseAgain('');
            setPassphraseError(null);
            setPassphraseDialogOpen(true);
          }}
          onRotateRecoveryCode={() => setWizardMode('rotate')}
          onTurnOff={() => setConfirmingTurnOff(true)}
        />
      ) : null}

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

      <ConfirmDialog
        isOpen={confirmingTurnOff}
        title="Turn off off-device backup?"
        message="Lattice stops writing archives to your folder."
        details="The archives already there stay where they are, and your recovery code still opens them. You can turn this back on without setting up a new code."
        variant="warning"
        confirmLabel="Turn off"
        onConfirm={() =>
          new Promise<void>((resolve, reject) => {
            disableArchive.mutate(undefined, {
              onSuccess: () => {
                setConfirmingTurnOff(false);
                resolve();
              },
              onError: (error) => reject(error),
            });
          })
        }
        onCancel={() => setConfirmingTurnOff(false)}
      />

      <ArchiveSetupWizard
        open={wizardMode !== null}
        mode={wizardMode ?? 'setup'}
        status={archive ?? null}
        onClose={() => setWizardMode(null)}
      />

      <Dialog
        open={secretPrompt}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) {
            setSecretPrompt(false);
            setSecretError(null);
          }
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Unlock this backup</DialogTitle>
            <DialogDescription>
              This backup was made on another computer, so Lattice needs the secret it was
              encrypted with.
            </DialogDescription>
          </DialogHeader>

          <div>
            <label htmlFor="archive-restore-secret" className="block text-xs text-text-secondary">
              Passphrase or recovery code
            </label>
            <textarea
              id="archive-restore-secret"
              rows={3}
              autoComplete="off"
              spellCheck={false}
              value={secret}
              onChange={(event) => setSecret(event.target.value)}
              className="mt-1 w-full rounded-sm border border-border-default bg-bg px-2.5 py-2 font-mono text-xs leading-relaxed text-text-primary outline-none transition-colors duration-fast focus:border-accent"
            />
            <p className="mt-1 text-xs text-text-muted">
              Either works. Paste the 24 recovery words with spaces between them.
            </p>
          </div>

          {secretError ? (
            <p role="alert" className="text-sm text-danger">
              {secretError}
            </p>
          ) : null}

          <DialogFooter>
            <button
              type="button"
              onClick={() => {
                setSecretPrompt(false);
                setSecretError(null);
              }}
              className={GHOST_BUTTON_CLASS}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => runArchiveRestore(secret.trim())}
              disabled={secret.trim().length === 0 || isRestoringArchive}
              className={PRIMARY_BUTTON_CLASS}
            >
              {isRestoringArchive ? 'Unlocking…' : 'Unlock'}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={passphraseDialogOpen}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setPassphraseDialogOpen(false);
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {archive?.hasPassphrase ? 'Change passphrase' : 'Set a passphrase'}
            </DialogTitle>
            <DialogDescription>
              A passphrase is a shorter thing to remember than the 24-word recovery code. Lattice
              cannot reset it — the recovery code is what gets you back in.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-2">
            <div>
              <label
                htmlFor="archive-new-passphrase"
                className="block text-xs text-text-secondary"
              >
                Passphrase
              </label>
              <input
                id="archive-new-passphrase"
                type="password"
                autoComplete="new-password"
                value={newPassphrase}
                onChange={(event) => setNewPassphrase(event.target.value)}
                className="mt-1 h-8 w-full rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-none transition-colors duration-fast focus:border-accent"
              />
            </div>
            <div>
              <label
                htmlFor="archive-new-passphrase-again"
                className="block text-xs text-text-secondary"
              >
                Passphrase again
              </label>
              <input
                id="archive-new-passphrase-again"
                type="password"
                autoComplete="new-password"
                value={newPassphraseAgain}
                onChange={(event) => setNewPassphraseAgain(event.target.value)}
                className="mt-1 h-8 w-full rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-none transition-colors duration-fast focus:border-accent"
              />
            </div>
            <p className="text-xs text-text-muted">At least {MIN_PASSPHRASE_LENGTH} characters.</p>
          </div>

          {passphraseError ? (
            <p role="alert" className="text-sm text-danger">
              {passphraseError}
            </p>
          ) : null}

          <DialogFooter>
            {archive?.hasPassphrase ? (
              <button
                type="button"
                onClick={removePassphrase}
                disabled={setPassphrase.isPending}
                className={GHOST_BUTTON_CLASS}
              >
                Remove passphrase
              </button>
            ) : null}
            <button
              type="button"
              onClick={() => setPassphraseDialogOpen(false)}
              className={GHOST_BUTTON_CLASS}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={submitPassphrase}
              disabled={setPassphrase.isPending}
              className={PRIMARY_BUTTON_CLASS}
            >
              {setPassphrase.isPending ? 'Saving…' : 'Save'}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

interface OffDeviceBackupProps {
  archive: ArchiveStatus;
  isIndexing: boolean;
  isArchiving: boolean;
  isRestoring: boolean;
  isChangingDestination: boolean;
  isSavingKeepCount: boolean;
  hydrationNotice: string | null;
  onSetUp: () => void;
  onChangeDestination: () => void;
  onKeepCountChange: (_keepCount: number) => void;
  onBackUpNow: () => void;
  onRestore: () => void;
  onRetryHydration: () => void;
  onManagePassphrase: () => void;
  onRotateRecoveryCode: () => void;
  onTurnOff: () => void;
}

function OffDeviceBackup({
  archive,
  isIndexing,
  isArchiving,
  isRestoring,
  isChangingDestination,
  isSavingKeepCount,
  hydrationNotice,
  onSetUp,
  onChangeDestination,
  onKeepCountChange,
  onBackUpNow,
  onRestore,
  onRetryHydration,
  onManagePassphrase,
  onRotateRecoveryCode,
  onTurnOff,
}: OffDeviceBackupProps) {
  const lastSuccessDate = archive.lastSuccess
    ? formatBackupDate(archive.lastSuccess.createdAt)
    : null;
  const lastSuccessSize = archive.lastSuccess ? formatBytes(archive.lastSuccess.size) : '';
  const keepCountOptions = KEEP_COUNT_OPTIONS.includes(archive.keepCount)
    ? KEEP_COUNT_OPTIONS
    : [...KEEP_COUNT_OPTIONS, archive.keepCount].sort((a, b) => a - b);

  return (
    <SettingsSection title="Off-device backup">
      {archive.dataDirCloudProvider ? (
        <div
          role="status"
          className="border-b border-border-subtle py-3 text-sm text-warning-fg"
        >
          Your Lattice database itself is inside {archive.dataDirCloudProvider}. A synced folder
          can corrupt an open database. Move Lattice's data folder out of it and use off-device
          backup instead — that writes one finished file at a time, which is safe to sync.
        </div>
      ) : null}

      {!archive.configured ? (
        <div className="border-b border-border-subtle py-3">
          <p className="text-sm text-text-secondary">
            Lattice can write one encrypted file of your whole library to a folder you choose — a
            cloud folder, a NAS, or a USB stick. Setup gives you a 24-word recovery code, which is
            the only way back in if you forget the passphrase. Lattice cannot reset it.
          </p>
          <button type="button" onClick={onSetUp} className={`${SECONDARY_BUTTON_CLASS} mt-3`}>
            Set up encrypted backup
          </button>
        </div>
      ) : (
        <>
          <SettingsRow label="Folder" stacked>
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-primary">
                {archive.destination ?? 'No folder chosen yet.'}
              </span>
              {archive.destinationProvider ? (
                <Badge variant="outline">{archive.destinationProvider}</Badge>
              ) : null}
              <button
                type="button"
                onClick={onChangeDestination}
                disabled={isChangingDestination}
                className={SECONDARY_BUTTON_CLASS}
              >
                {archive.destination ? 'Change' : 'Choose folder'}
              </button>
            </div>
            {archive.destinationMissing ? (
              <p className="mt-1.5 text-xs text-danger">
                That folder is gone. Choose another one, or reconnect the drive.
              </p>
            ) : null}
          </SettingsRow>

          <SettingsRow
            label="Archives to keep"
            hint="Older archives in the folder are deleted after a successful backup."
            htmlFor="archive-keep-count"
          >
            <select
              id="archive-keep-count"
              value={archive.keepCount}
              disabled={isSavingKeepCount}
              onChange={(event) => onKeepCountChange(Number(event.target.value))}
              className={settingsFieldClass}
            >
              {keepCountOptions.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
          </SettingsRow>

          <SettingsRow label="Passphrase" hint="Optional. The recovery code always works.">
            <button type="button" onClick={onManagePassphrase} className={SECONDARY_BUTTON_CLASS}>
              {archive.hasPassphrase ? 'Change passphrase' : 'Set passphrase'}
            </button>
          </SettingsRow>

          <div className="border-b border-border-subtle py-3">
            <p className="text-sm text-text-primary">
              {lastSuccessDate
                ? `Last backup ${lastSuccessDate}${lastSuccessSize ? ` · ${lastSuccessSize}` : ''}`
                : 'No backup written yet.'}
            </p>
            {archive.lastError ? (
              <p className="mt-0.5 text-sm text-danger">
                Last attempt failed: {archive.lastError.message}
              </p>
            ) : null}
            {hydrationNotice ? (
              <div className="mt-2 flex items-start gap-3">
                <p role="status" className="min-w-0 flex-1 text-sm text-text-secondary">
                  {hydrationNotice}
                </p>
                <button
                  type="button"
                  onClick={onRetryHydration}
                  disabled={isRestoring}
                  className={SECONDARY_BUTTON_CLASS}
                >
                  Try again
                </button>
              </div>
            ) : null}
          </div>

          {archive.archives.length > 0 ? (
            <div className="border-b border-border-subtle py-1">
              {archive.archives.map((file) => {
                const date = formatBackupDate(file.createdAt);
                const size = formatBytes(file.size);
                return (
                  <div key={file.path} className="flex items-baseline gap-3 py-1.5">
                    {date ? (
                      <span className="truncate text-sm text-text-primary">{date}</span>
                    ) : (
                      <span className="truncate font-mono text-xs text-text-primary">
                        {file.name}
                      </span>
                    )}
                    {size ? (
                      <span className="shrink-0 text-xs tabular-nums text-text-muted">{size}</span>
                    ) : null}
                    {file.availability === 'placeholder' ? (
                      <span className="shrink-0 text-xs text-text-muted">not downloaded</span>
                    ) : null}
                  </div>
                );
              })}
            </div>
          ) : null}

          <div className="flex flex-wrap items-center gap-2 py-3">
            <button
              type="button"
              onClick={onBackUpNow}
              disabled={isArchiving || isIndexing || !archive.destination}
              title={isIndexing ? 'Wait for indexing to finish before backing up.' : undefined}
              className={SECONDARY_BUTTON_CLASS}
            >
              {isArchiving ? 'Backing up…' : 'Back up now'}
            </button>
            <button
              type="button"
              onClick={onRestore}
              disabled={isRestoring}
              className={SECONDARY_BUTTON_CLASS}
            >
              {isRestoring ? 'Restoring…' : 'Restore from file'}
            </button>
            <button type="button" onClick={onRotateRecoveryCode} className={GHOST_BUTTON_CLASS}>
              Generate new recovery code
            </button>
            <button type="button" onClick={onTurnOff} className={GHOST_BUTTON_CLASS}>
              Turn off
            </button>
          </div>
        </>
      )}
    </SettingsSection>
  );
}
