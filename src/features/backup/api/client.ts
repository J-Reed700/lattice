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
  createBackup: async (): Promise<ApiResult<CreateBackupResult>> =>
    apiCall<Wire.CreateBackupResultDto>('plugin_create_backup', {
      request: { backupPath: null },
    }),

  listBackups: async (): Promise<ApiResult<BackupInfo[]>> => {
    const result = await apiCall<Wire.ListBackupsResultDto>(
      'plugin_list_backups',
    );
    if (!result.ok) return result;
    return { ok: true, data: result.data.backups ?? [] };
  },

  /** Replaces the live database with a backup; app must be restarted afterwards. */
  restoreBackup: async (
    backupPath: string,
  ): Promise<ApiResult<RestoreBackupResult>> =>
    apiCall<Wire.RestoreBackupResultDto>('plugin_restore_backup', {
      request: { backupPath },
    }),

  exportMarkdown: async (): Promise<ApiResult<ExportSummary>> =>
    apiCall<Wire.ExportResultDto>('plugin_export_markdown', {
      request: { outputDir: null },
    }),

  exportJson: async (): Promise<ApiResult<ExportSummary>> =>
    apiCall<Wire.ExportResultDto>('plugin_export_json', {
      request: { outputPath: null, pretty: true },
    }),

  getArchiveStatus: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_get_archive_status') as Promise<
      ApiResult<ArchiveStatus>
    >,

  beginArchiveSetup: async (): Promise<ApiResult<ArchiveSetup>> =>
    apiCall<Wire.ArchiveSetupDto>('plugin_begin_archive_setup'),

  confirmArchiveSetup: async (
    confirmations: WordConfirmation[],
    passphrase: string | null,
  ): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_confirm_archive_setup', {
      request: { confirmations, passphrase },
    }) as Promise<ApiResult<ArchiveStatus>>,

  chooseArchiveDestination: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>(
      'plugin_choose_archive_destination',
    ) as Promise<ApiResult<ArchiveStatus>>,

  setArchiveKeepCount: async (
    keepCount: number,
  ): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_set_archive_keep_count', {
      request: { keepCount },
    }) as Promise<ApiResult<ArchiveStatus>>,

  setArchivePassphrase: async (
    passphrase: string | null,
  ): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_set_archive_passphrase', {
      request: { passphrase },
    }) as Promise<ApiResult<ArchiveStatus>>,

  rotateRecoveryCode: async (): Promise<ApiResult<ArchiveSetup>> =>
    apiCall<Wire.ArchiveSetupDto>('plugin_rotate_recovery_code'),

  disableArchive: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_disable_archive') as Promise<
      ApiResult<ArchiveStatus>
    >,

  createArchiveNow: async (): Promise<ApiResult<ArchiveRun>> =>
    apiCall<Wire.ArchiveRunDto>('plugin_create_archive_now'),

  restoreArchive: async (
    secret: string | null,
  ): Promise<ApiResult<RestoreArchiveResult>> =>
    apiCall<Wire.RestoreArchiveResultDto>('plugin_restore_archive', {
      request: { secret },
    }) as Promise<ApiResult<RestoreArchiveResult>>,
};
