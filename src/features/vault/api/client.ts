import type { VaultConflictDto } from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';

export const vaultApi = {
  /**
   * Rescan the vault folder for external `.md` edits the watcher may have
   * missed. No-op if vault is disabled or watch toggle is off (backend
   * gates). Returns counts of files scanned and imported.
   *
   * Called from the focus-rescan hook whenever the Tauri window regains
   * focus — belt-and-suspenders for the fs watcher.
   */
  rescanVault: async (): Promise<
    ApiResult<{ scanned: number; imported: number; deleted: number }>
  > => apiCall('rescan_vault'),

  /** Unacknowledged vault conflicts recorded in SQLite. */
  listVaultConflicts: async (): Promise<ApiResult<VaultConflictDto[]>> =>
    apiCall('list_vault_conflicts'),

  /** Mark the given vault conflict notices as read/dismissed. */
  dismissVaultConflicts: async (ids: string[]): Promise<ApiResult<null>> =>
    apiCall('dismiss_vault_conflicts', { ids }),
};
