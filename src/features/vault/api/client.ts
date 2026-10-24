import type * as Wire from '@/lib/bindings';
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
  > => apiCall<Wire.RescanSummary>('rescan_vault'),
};
