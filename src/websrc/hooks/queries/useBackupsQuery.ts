import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { BackupInfo, CreateBackupResult, ExportSummary } from '@/types/api/backup';

export const BACKUPS_QUERY_KEY = ['backups', 'list'] as const;

/** Backups on disk, newest first. */
export function useBackupsQuery() {
  return useQuery<BackupInfo[]>({
    queryKey: BACKUPS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.listBackups();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 30_000,
  });
}

export function useCreateBackupMutation() {
  const queryClient = useQueryClient();

  return useMutation<CreateBackupResult, Error, void>({
    mutationFn: async () => {
      const result = await VaultAPI.createBackup();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: BACKUPS_QUERY_KEY });
    },
  });
}

export function useRestoreBackupMutation() {
  const queryClient = useQueryClient();

  return useMutation<void, Error, string>({
    mutationFn: async (backupPath) => {
      const result = await VaultAPI.restoreBackup(backupPath);
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: BACKUPS_QUERY_KEY });
    },
  });
}

export type ExportFormat = 'markdown' | 'json';

export function useExportMutation() {
  return useMutation<ExportSummary, Error, ExportFormat>({
    mutationFn: async (format) => {
      const result =
        format === 'markdown' ? await VaultAPI.exportMarkdown() : await VaultAPI.exportJson();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
  });
}
