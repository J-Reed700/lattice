import { useMutation, useQuery } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { UpdateInfo, VersionInfo } from '@/types/api/updates';

export const VERSION_QUERY_KEY = ['app', 'version'] as const;

/** The running build. Never changes while the app is open. */
export function useVersionInfoQuery() {
  return useQuery<VersionInfo>({
    queryKey: VERSION_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getVersionInfo();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: Infinity,
  });
}

/**
 * A mutation, not a query. Checking for updates is a user-initiated network
 * call with a rate limit behind it; it must never fire on mount, on focus, or
 * on an interval.
 */
export function useCheckForUpdatesMutation() {
  return useMutation<UpdateInfo, Error, void>({
    mutationFn: async () => {
      const result = await VaultAPI.checkForUpdates();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
  });
}
