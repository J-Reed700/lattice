import { useMutation, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { INDEXED_FOLDERS_QUERY_KEY } from '@/features/files/hooks/useIndexedFoldersQuery';
import { LIBRARY_DOCUMENTS_QUERY_KEY } from '@/features/files/hooks/useLibraryDocumentsQuery';

/**
 * Adds files and folders to the library by path. Each answers with the
 * command's result; progress arrives through the indexing status listeners.
 */
export function useAddToLibrary() {
  const api = useApi();
  const queryClient = useQueryClient();
  const file = useMutation({
    mutationFn: (path: string) => api.indexFile(path),
    onSuccess: async (result) => {
      if (!result.ok) return;
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: LIBRARY_DOCUMENTS_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: ['dashboard'] }),
        queryClient.invalidateQueries({ queryKey: ['spaces', 'documents'] }),
      ]);
    },
  });
  const folder = useMutation({
    mutationFn: (path: string) => api.startIndexing(path, true),
    onSuccess: async (result) => {
      if (!result.ok) return;
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: INDEXED_FOLDERS_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: LIBRARY_DOCUMENTS_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: ['dashboard'] }),
        queryClient.invalidateQueries({ queryKey: ['spaces', 'documents'] }),
      ]);
    },
  });
  return { indexFile: file.mutateAsync, indexFolder: folder.mutateAsync };
}
