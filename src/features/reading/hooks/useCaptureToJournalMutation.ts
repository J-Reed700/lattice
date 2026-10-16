import { useMutation } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';

/**
 * Appends a passage to the journal page the user captures into. Answers with
 * the command's result, which names the page the capture landed on.
 */
export function useCaptureToJournalMutation() {
  return useMutation({ mutationFn: (content: string) => VaultAPI.quickCapture(content) });
}
