import { Badge, Button, SettingsRow, SettingsSection } from '@/components/ui';
import type { ArchiveStatus } from '@/types/api/backup';

import { formatBackupDate, formatBytes } from './backupFormat';
import { SettingsSelect } from './SettingControls';

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
          can corrupt an open database. Move Lattice&apos;s data folder out of it and use off-device
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
          <Button type="button" variant="outline" onClick={onSetUp} className="mt-3">
            Set up encrypted backup
          </Button>
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
              <Button
                type="button"
                variant="outline"
                onClick={onChangeDestination}
                disabled={isChangingDestination}
              >
                {archive.destination ? 'Change' : 'Choose folder'}
              </Button>
            </div>
            {archive.destinationMissing ? (
              <p className="mt-1.5 text-xs text-danger-fg">
                That folder is gone. Choose another one, or reconnect the drive.
              </p>
            ) : null}
          </SettingsRow>

          <SettingsRow
            label="Archives to keep"
            hint="Older archives in the folder are deleted after a successful backup."
            htmlFor="archive-keep-count"
          >
            <div className="w-28">
              <SettingsSelect
                id="archive-keep-count"
                value={String(archive.keepCount)}
                disabled={isSavingKeepCount}
                onValueChange={(next) => onKeepCountChange(Number(next))}
                options={keepCountOptions.map((option) => ({
                  value: String(option),
                  label: String(option),
                }))}
              />
            </div>
          </SettingsRow>

          <SettingsRow label="Passphrase" hint="Optional. The recovery code always works.">
            <Button type="button" variant="outline" onClick={onManagePassphrase}>
              {archive.hasPassphrase ? 'Change passphrase' : 'Set passphrase'}
            </Button>
          </SettingsRow>

          <div className="border-b border-border-subtle py-3">
            <p className="text-sm text-text-primary">
              {lastSuccessDate
                ? `Last backup ${lastSuccessDate}${lastSuccessSize ? ` · ${lastSuccessSize}` : ''}`
                : 'No backup written yet.'}
            </p>
            {archive.lastError ? (
              <p className="mt-0.5 text-sm text-danger-fg">
                Last attempt failed: {archive.lastError.message}
              </p>
            ) : null}
            {hydrationNotice ? (
              <div className="mt-2 flex items-start gap-3">
                <p role="status" className="min-w-0 flex-1 text-sm text-text-secondary">
                  {hydrationNotice}
                </p>
                <Button
                  type="button"
                  variant="outline"
                  onClick={onRetryHydration}
                  disabled={isRestoring}
                >
                  Try again
                </Button>
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
            <Button
              type="button"
              variant="outline"
              onClick={onBackUpNow}
              disabled={isArchiving || isIndexing || !archive.destination}
              title={isIndexing ? 'Wait for indexing to finish before backing up.' : undefined}
            >
              {isArchiving ? 'Backing up…' : 'Back up now'}
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={onRestore}
              disabled={isRestoring}
            >
              {isRestoring ? 'Restoring…' : 'Restore from file'}
            </Button>
            <Button type="button" variant="ghost" onClick={onRotateRecoveryCode}>
              Generate new recovery code
            </Button>
            <Button type="button" variant="ghost" onClick={onTurnOff}>
              Turn off
            </Button>
          </div>
        </>
      )}
    </SettingsSection>
  );
}
