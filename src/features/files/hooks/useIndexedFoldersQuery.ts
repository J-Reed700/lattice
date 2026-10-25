import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { SETTINGS_QUERY_KEY } from '@/features/settings/hooks/useSettingsQuery';
import type { IndexedFolder } from '@/types/api/files';


export const INDEXED_FOLDERS_QUERY_KEY = ['indexed-folders'] as const;

export function useIndexedFoldersQuery() {
  const api = useApi();
  return useQuery<IndexedFolder[]>({
    queryKey: INDEXED_FOLDERS_QUERY_KEY,
    queryFn: async () => {
      const result = await api.getIndexedFolders();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 30_000,
  });
}

interface SetWatchFolderEnabledArgs {
  path: string;
  enabled: boolean;
}

export function useSetWatchFolderEnabledMutation() {
  const api = useApi();
  const queryClient = useQueryClient();

  return useMutation<void, Error, SetWatchFolderEnabledArgs>({
    mutationFn: async ({ path, enabled }) => {
      const result = enabled
        ? await api.addWatchFolder(path)
        : await api.removeWatchFolder(path);
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: INDEXED_FOLDERS_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY }),
      ]);
    },
  });
}
