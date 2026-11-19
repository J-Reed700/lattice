import { useCallback, useContext } from 'react';

import { QueryClientContext } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { WORKSPACE_NOTES_QUERY_KEY } from '@/lib/queryClient';

/**
 * Append markdown to the journal page written to last. The result names the
 * page, so the caller can say where the capture landed and offer to open it.
 */
export function useQuickCapture() {
  const api = useApi();
  const queryClient = useContext(QueryClientContext);
  return useCallback(
    async (content: string) => {
      const result = await api.quickCapture(content);
      if (result.ok && queryClient) {
        await queryClient.invalidateQueries({ queryKey: WORKSPACE_NOTES_QUERY_KEY });
      }
      return result;
    },
    [api, queryClient],
  );
}
