import { useCallback } from 'react';

import { useApi } from '@/contexts/ApiContext';



/** Bringing a web page into the library: a look before, and the import. */
export function useWebImport() {
  const api = useApi();
  const preview = useCallback((url: string) => api.fetchUrlPreview(url), [api]);
  const importPage = useCallback((url: string) => api.ingestWebUrl(url), [api]);
  return { preview, importPage };
}
