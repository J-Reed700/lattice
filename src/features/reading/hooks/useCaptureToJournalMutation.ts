import { useMutation, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { WORKSPACE_NOTES_QUERY_KEY } from '@/lib/queryClient';
import { unwrapApiResult } from '@/types/api/result';

/**
 * Appends a passage to the journal page the user captures into. Answers with
 * the saved note reference, which names the page the capture landed on.
 */
export function useCaptureToJournalMutation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (content: string) => unwrapApiResult(await api.quickCapture(content)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: WORKSPACE_NOTES_QUERY_KEY }),
  });
}
