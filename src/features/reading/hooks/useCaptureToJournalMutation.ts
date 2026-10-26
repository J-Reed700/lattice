import { useMutation } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';



/**
 * Appends a passage to the journal page the user captures into. Answers with
 * the command's result, which names the page the capture landed on.
 */
export function useCaptureToJournalMutation() {
  const api = useApi();
  return useMutation({ mutationFn: (content: string) => api.quickCapture(content) });
}
