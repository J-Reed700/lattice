import { useCallback } from 'react';

import { useApi } from '@/contexts/ApiContext';



/** Rename a document's display name; the file on disk keeps its own. */
export function useRenameDocument() {
  const api = useApi();
  return useCallback((documentId: string, name: string) => api.renameDocument(documentId, name), [api]);
}
