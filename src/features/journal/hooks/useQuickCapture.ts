import { useCallback } from 'react';

import { VaultAPI } from '@/lib/api';

/**
 * Append markdown to the journal page written to last. The result names the
 * page, so the caller can say where the capture landed and offer to open it.
 */
export function useQuickCapture() {
  return useCallback((content: string) => VaultAPI.quickCapture(content), []);
}
