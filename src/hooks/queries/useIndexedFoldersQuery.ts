import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { IndexedFolder } from '@/types/api/files';

import { SETTINGS_QUERY_KEY } from './useSettingsQuery';

export const INDEXED_FOLDERS_QUERY_KEY = ['indexed-folders'] as const;

export function useIndexedFoldersQuery() {
  return useQuery<IndexedFolder[]>({
    queryKey: INDEXED_FOLDERS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getIndexedFolders();
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
  const queryClient = useQueryClient();

  return useMutation<void, Error, SetWatchFolderEnabledArgs>({
    mutationFn: async ({ path, enabled }) => {
      const result = enabled
        ? await VaultAPI.addWatchFolder(path)
        : await VaultAPI.removeWatchFolder(path);
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
