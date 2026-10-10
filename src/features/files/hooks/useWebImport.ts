import { useCallback } from 'react';

import { VaultAPI } from '@/lib/api';

/** Bringing a web page into the library: a look before, and the import. */
export function useWebImport() {
  const preview = useCallback((url: string) => VaultAPI.fetchUrlPreview(url), []);
  const importPage = useCallback((url: string) => VaultAPI.ingestWebUrl(url), []);
  return { preview, importPage };
}
