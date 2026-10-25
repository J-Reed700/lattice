import { useMutation, useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import type { UpdateInfo, VersionInfo } from '@/types/api/updates';

export const VERSION_QUERY_KEY = ['app', 'version'] as const;

/** The running build. Never changes while the app is open. */
export function useVersionInfoQuery() {
  const api = useApi();
  return useQuery<VersionInfo>({
    queryKey: VERSION_QUERY_KEY,
    queryFn: async () => {
      const result = await api.getVersionInfo();
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
 * call; it must never fire on mount, on focus, or on an interval.
 */
export function useCheckForUpdatesMutation() {
  const api = useApi();
  return useMutation<UpdateInfo, Error, void>({
    mutationFn: async () => {
      const result = await api.checkForUpdates();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
  });
}
