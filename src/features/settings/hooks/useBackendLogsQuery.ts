import { useQuery } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { BackendLogEntry } from '@/lib/bindings';

export const BACKEND_LOGS_QUERY_KEY = ['logs', 'backend'] as const;

/**
 * The newest lines of the backend's log file, oldest first. Read when the
 * Logs tab opens and on `refetch`; never polled.
 */
export function useBackendLogsQuery() {
  return useQuery<BackendLogEntry[]>({
    queryKey: BACKEND_LOGS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getBackendLogs();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });
}
