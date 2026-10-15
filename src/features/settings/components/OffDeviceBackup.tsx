import { SettingsRow, SettingsSection, settingsFieldClass } from '@/components/ui';
import { Badge } from '@/components/ui/badge';
import type { ArchiveStatus } from '@/types/api/backup';

import { formatBackupDate, formatBytes } from './backupFormat';
import { GHOST_BUTTON_CLASS, SECONDARY_BUTTON_CLASS } from './settingsStyles';

const KEEP_COUNT_OPTIONS = [1, 3, 5, 10, 20];

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

export function OffDeviceBackup({
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
