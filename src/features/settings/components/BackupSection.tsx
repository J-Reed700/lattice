import { useMemo, useState } from 'react';

import { Archive, Download, Upload } from 'lucide-react';

import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { SettingsRow, SettingsSection } from '@/components/ui';
import {
  isIndexingActive,
  useIndexingStatusQuery,
} from '@/features/files/hooks/useIndexingStatusQuery';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import { toast } from '@/stores/toastStore';
import type { BackupInfo } from '@/types/api/backup';

import {
  MIN_PASSPHRASE_LENGTH,
  PassphraseDialog,
  RestoreSecretDialog,
} from './ArchiveCredentialDialogs';
import { ArchiveSetupWizard, type ArchiveWizardMode } from './ArchiveSetupWizard';
import { formatBackupDate, formatBytes } from './backupFormat';
import { OffDeviceBackup } from './OffDeviceBackup';
import {
  GHOST_BUTTON_CLASS,
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
} from '../hooks/useArchiveQuery';
import { useBackupAndExport } from '../hooks/useBackupAndExport';
import {
  useBackupsQuery,
  useRestoreBackupMutation,
} from '../hooks/useBackupsQuery';

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
  const { createBackup, exportData, runBackup, runExport, showInFolder } = useBackupAndExport();
  const restoreBackup = useRestoreBackupMutation();
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
                    onClick={() => void showInFolder(backup.path)}
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

      <RestoreSecretDialog
        open={secretPrompt}
        secret={secret}
        error={secretError}
        isRestoring={isRestoringArchive}
        onSecretChange={setSecret}
        onDismiss={() => {
          setSecretPrompt(false);
          setSecretError(null);
        }}
        onUnlock={runArchiveRestore}
      />

      <PassphraseDialog
        open={passphraseDialogOpen}
        hasPassphrase={archive?.hasPassphrase}
        newPassphrase={newPassphrase}
        newPassphraseAgain={newPassphraseAgain}
        error={passphraseError}
        isSaving={setPassphrase.isPending}
        onNewPassphraseChange={setNewPassphrase}
        onNewPassphraseAgainChange={setNewPassphraseAgain}
        onClose={() => setPassphraseDialogOpen(false)}
        onRemove={removePassphrase}
        onSubmit={submitPassphrase}
      />
    </>
  );
}
