import { useCallback } from 'react';

import { useApi } from '@/contexts/ApiContext';



/**
 * Append markdown to the journal page written to last. The result names the
 * page, so the caller can say where the capture landed and offer to open it.
 */
export function useQuickCapture() {
  const api = useApi();
  return useCallback((content: string) => api.quickCapture(content), [api]);
}
