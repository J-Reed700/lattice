import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  BackupInfo,
  CreateBackupResult,
  RestoreBackupResult,
  ExportSummary,
  ArchiveStatus,
  ArchiveSetup,
  ArchiveRun,
  RestoreArchiveResult,
  WordConfirmation,
} from '@/types';

export const backupApi = {
  /**
   * Writes a database backup into the app's backups folder and returns its path.
   * The destination is fixed by the backend (CWE-22 confinement); there is no
   * folder argument to pass.
   */
  createBackup: async (): Promise<ApiResult<CreateBackupResult>> =>
    apiCall<Wire.CreateBackupResultDto>('plugin_create_backup', {
      request: { backupPath: null },
    }),

  /** Backups on disk, newest first. */
  listBackups: async (): Promise<ApiResult<BackupInfo[]>> => {
    const result = await apiCall<Wire.ListBackupsResultDto>(
      'plugin_list_backups',
    );
    if (!result.ok) return result;
    return { ok: true, data: result.data.backups ?? [] };
  },

  /**
   * Replaces the live database with a backup. The backend closes the connection
   * pool and does not reopen it — the app must be quit and reopened afterwards.
   */
  restoreBackup: async (
    backupPath: string,
  ): Promise<ApiResult<RestoreBackupResult>> =>
    apiCall<Wire.RestoreBackupResultDto>('plugin_restore_backup', {
      request: { backupPath },
    }),

  /** Exports conversations and journal pages as Markdown into the app's exports folder. */
  exportMarkdown: async (): Promise<ApiResult<ExportSummary>> =>
    apiCall<Wire.ExportResultDto>('plugin_export_markdown', {
      request: { outputDir: null },
    }),

  /** Exports conversations and journal pages as one JSON file in the app's exports folder. */
  exportJson: async (): Promise<ApiResult<ExportSummary>> =>
    apiCall<Wire.ExportResultDto>('plugin_export_json', {
      request: { outputPath: null, pretty: true },
    }),

  /* ---------------------------------------------------------------- *
   * Off-device backup (encrypted archive).
   *
   * Every path in this group is chosen by a native picker on the Rust side —
   * the webview neither sends nor receives a folder to write to. The generic
   * on each `apiCall` is the local DTO type in `types/api/backup.ts` rather
   * than a `Wire.*` type, because these commands are added to
   * `lib/bindings.ts` by `npm run bindings:generate` on the Rust side.
   * ---------------------------------------------------------------- */

  /** Current archive configuration, last run, and the archives on disk. */
  getArchiveStatus: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_get_archive_status') as Promise<
      ApiResult<ArchiveStatus>
    >,

  /**
   * Generates a master key and a 24-word recovery code, held in memory only.
   * Nothing is written until `confirmArchiveSetup` succeeds; calling again
   * throws the previous pending setup away.
   */
  beginArchiveSetup: async (): Promise<ApiResult<ArchiveSetup>> =>
    apiCall<Wire.ArchiveSetupDto>('plugin_begin_archive_setup'),

  /**
   * Proves the user wrote the recovery code down, then persists the envelope.
   * `passphrase` is optional and must be at least 8 characters when present.
   */
  confirmArchiveSetup: async (
    confirmations: WordConfirmation[],
    passphrase: string | null,
  ): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_confirm_archive_setup', {
      request: { confirmations, passphrase },
    }) as Promise<ApiResult<ArchiveStatus>>,

  /** Opens the Rust-side folder picker. Cancelling returns the status unchanged. */
  chooseArchiveDestination: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>(
      'plugin_choose_archive_destination',
    ) as Promise<ApiResult<ArchiveStatus>>,

  /** How many archives to keep in the destination folder (1–50). */
  setArchiveKeepCount: async (
    keepCount: number,
  ): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_set_archive_keep_count', {
      request: { keepCount },
    }) as Promise<ApiResult<ArchiveStatus>>,

  /** Sets, changes, or (with `null`) removes the passphrase slot. */
  setArchivePassphrase: async (
    passphrase: string | null,
  ): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_set_archive_passphrase', {
      request: { passphrase },
    }) as Promise<ApiResult<ArchiveStatus>>,

  /**
   * Issues a new recovery code. Confirm it with `confirmArchiveSetup`; the
   * passphrase slot is untouched, so pass `null` for the passphrase there.
   */
  rotateRecoveryCode: async (): Promise<ApiResult<ArchiveSetup>> =>
    apiCall<Wire.ArchiveSetupDto>('plugin_rotate_recovery_code'),

  /** Stops writing archives. The key and the envelope stay, so it can be turned back on. */
  disableArchive: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_disable_archive') as Promise<
      ApiResult<ArchiveStatus>
    >,

  /** Writes one archive now, outside the schedule. */
  createArchiveNow: async (): Promise<ApiResult<ArchiveRun>> =>
    apiCall<Wire.ArchiveRunDto>('plugin_create_archive_now'),

  /**
   * Restores from an archive the user picks in the Rust-side file dialog.
   * Pass `null` first: the keyring usually holds the key already. A
   * `needs_secret` outcome means asking for the passphrase or the recovery
   * code and calling again with it.
   */
  restoreArchive: async (
    secret: string | null,
  ): Promise<ApiResult<RestoreArchiveResult>> =>
    apiCall<Wire.RestoreArchiveResultDto>('plugin_restore_archive', {
      request: { secret },
    }) as Promise<ApiResult<RestoreArchiveResult>>,
};
