/**
 * Off-device backup (encrypted archive) queries and mutations.
 *
 * One status query is the single source of truth for the whole block — every
 * command on the Rust side returns the fresh `ArchiveStatusDto`, so each
 * mutation writes that straight into the cache and then invalidates, which
 * keeps the UI correct even when two surfaces are mounted at once.
 */

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

/** Archive configuration, last run, and the archives in the destination folder. */
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

/**
 * Shared tail for every command that hands back a status: seed the cache so
 * the UI updates in the same tick, then refetch in case the folder changed
 * underneath us.
 */
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

/** Starts a setup run. The words exist only in memory until confirmation. */
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

/** Issues a replacement recovery code for an already-configured archive. */
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
  /** `null` when the step was skipped or the wizard is rotating the code. */
  passphrase: string | null;
}

/** Verifies the three words and writes the envelope. */
export function useConfirmArchiveSetupMutation() {
  return useStatusMutation<ConfirmArchiveSetupVariables>(async ({ confirmations, passphrase }) => {
    const result = await VaultAPI.confirmArchiveSetup(confirmations, passphrase);
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

/** Opens the native folder picker on the Rust side. */
export function useChooseArchiveDestinationMutation() {
  return useStatusMutation<void>(async () => {
    const result = await VaultAPI.chooseArchiveDestination();
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

/** How many archives to keep in the destination folder. */
export function useSetArchiveKeepCountMutation() {
  return useStatusMutation<number>(async (keepCount) => {
    const result = await VaultAPI.setArchiveKeepCount(keepCount);
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

/** Sets, changes, or removes (`null`) the passphrase. */
export function useSetArchivePassphraseMutation() {
  return useStatusMutation<string | null>(async (passphrase) => {
    const result = await VaultAPI.setArchivePassphrase(passphrase);
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

/** Stops writing archives. The key and the envelope stay. */
export function useDisableArchiveMutation() {
  return useStatusMutation<void>(async () => {
    const result = await VaultAPI.disableArchive();
    if (!result.ok) {
      throw new Error(result.error);
    }
    return result.data;
  });
}

/** Writes one archive now. Invalidates the status so the new file shows up. */
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
      // A failed run is recorded in `lastError`, so refetch either way.
      void queryClient.invalidateQueries({ queryKey: ARCHIVE_STATUS_QUERY_KEY });
    },
  });
}

/**
 * Restores from an archive file the user picks on the Rust side.
 *
 * Returns the whole result rather than throwing on a non-`restored` outcome:
 * `needs_secret`, `not_hydrated`, and `cancelled` are all normal answers the
 * caller has to branch on. Only a genuine failure rejects.
 */
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
