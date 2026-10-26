import { useMutation } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';



/**
 * Adds files and folders to the library by path. Each answers with the
 * command's result; progress arrives through the indexing status listeners.
 */
export function useAddToLibrary() {
  const api = useApi();
  const file = useMutation({ mutationFn: (path: string) => api.indexFile(path) });
  const folder = useMutation({ mutationFn: (path: string) => api.startIndexing(path, true) });
  return { indexFile: file.mutateAsync, indexFolder: folder.mutateAsync };
}
