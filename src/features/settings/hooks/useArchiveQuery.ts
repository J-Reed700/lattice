import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type {
  ArchiveRun,
  ArchiveSetup,
  ArchiveStatus,
  RestoreArchiveResult,
  WordConfirmation,
} from '@/types/api/backup';

import { BACKUPS_QUERY_KEY } from './useBackupsQuery';

export const ARCHIVE_STATUS_QUERY_KEY = ['backups', 'archive', 'status'] as const;

export function useArchiveStatusQuery() {
  return useQuery<ArchiveStatus>({
    queryKey: ARCHIVE_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getArchiveStatus();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 30_000,
  });
}

function useStatusMutation<TVariables>(
  run: (variables: TVariables) => Promise<ArchiveStatus>,
) {
  const queryClient = useQueryClient();

  return useMutation<ArchiveStatus, Error, TVariables>({
    mutationFn: run,
    onSuccess: (status) => {
      queryClient.setQueryData(ARCHIVE_STATUS_QUERY_KEY, status);
      void queryClient.invalidateQueries({ queryKey: ARCHIVE_STATUS_QUERY_KEY });
    },
  });
}

export function useBeginArchiveSetupMutation() {
  return useMutation<ArchiveSetup, Error, void>({
    mutationFn: async () => {
      const result = await VaultAPI.beginArchiveSetup();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
  });
}

export function useRotateRecoveryCodeMutation() {
  return useMutation<ArchiveSetup, Error, void>({
    mutationFn: async () => {
      const result = await VaultAPI.rotateRecoveryCode();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
  });
}

export interface ConfirmArchiveSetupVariables {
  confirmations: WordConfirmation[];
  passphrase: string | null;
}

export function useConfirmArchiveSetupMutation() {
  return useStatusMutation<ConfirmArchiveSetupVariables>(async ({ confirmations, passphrase }) => {
    const result = await VaultAPI.confirmArchiveSetup(confirmations, passphrase);
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

export function useChooseArchiveDestinationMutation() {
  return useStatusMutation<void>(async () => {
    const result = await VaultAPI.chooseArchiveDestination();
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

export function useSetArchiveKeepCountMutation() {
  return useStatusMutation<number>(async (keepCount) => {
    const result = await VaultAPI.setArchiveKeepCount(keepCount);
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

export function useSetArchivePassphraseMutation() {
  return useStatusMutation<string | null>(async (passphrase) => {
    const result = await VaultAPI.setArchivePassphrase(passphrase);
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

export function useDisableArchiveMutation() {
  return useStatusMutation<void>(async () => {
    const result = await VaultAPI.disableArchive();
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

export function useCreateArchiveNowMutation() {
  const queryClient = useQueryClient();

  return useMutation<ArchiveRun, Error, void>({
    mutationFn: async () => {
      const result = await VaultAPI.createArchiveNow();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: ARCHIVE_STATUS_QUERY_KEY });
    },
  });
}

export function useRestoreArchiveMutation() {
  const queryClient = useQueryClient();

  return useMutation<RestoreArchiveResult, Error, string | null>({
    mutationFn: async (secret) => {
      const result = await VaultAPI.restoreArchive(secret);
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSuccess: (result) => {
      if (result.outcome !== 'restored') return;
      void queryClient.invalidateQueries({ queryKey: ARCHIVE_STATUS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: BACKUPS_QUERY_KEY });
    },
  });
}
