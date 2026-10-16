import { useMutation } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';

/**
 * Adds files and folders to the library by path. Each answers with the
 * command's result; progress arrives through the indexing status listeners.
 */
export function useAddToLibrary() {
  const file = useMutation({ mutationFn: (path: string) => VaultAPI.indexFile(path) });
  const folder = useMutation({ mutationFn: (path: string) => VaultAPI.startIndexing(path, true) });
  return { indexFile: file.mutateAsync, indexFolder: folder.mutateAsync };
}
