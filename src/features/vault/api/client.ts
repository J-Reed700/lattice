import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';

export const vaultApi = {
  /** Rescans the vault folder for external markdown edits missed by the watcher. */
  rescanVault: async (): Promise<
    ApiResult<{ scanned: number; imported: number; deleted: number }>
  > => apiCall<Wire.RescanSummary>('rescan_vault'),
};
