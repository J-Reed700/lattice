import { useCallback } from 'react';

import { VaultAPI } from '@/lib/api';

/** Rename a document's display name; the file on disk keeps its own. */
export function useRenameDocument() {
  return useCallback((documentId: string, name: string) => VaultAPI.renameDocument(documentId, name), []);
}
