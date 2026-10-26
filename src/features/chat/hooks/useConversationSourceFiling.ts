import { useMutation } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';

export interface IngestWebSourceArgs {
  url: string;
  spaceId?: string;
  /** Leave the linked documents alone; a batch reloads them once at its end. */
  skipRefresh?: boolean;
}

/**
 * Files a conversation's attachments into the library and indexes its web
 * sources. Each answers with the command's result so the view can say why it
 * failed; a success reloads the conversation's linked documents first.
 */
export function useConversationSourceFiling(conversationId: string) {
  const api = useApi();
  const loadLinkedDocuments = useConversationsStore((state) => state.loadConversationLinkedDocuments);
  const addToLibrary = useMutation({
    mutationFn: (documentIds: string[]) => api.addDocumentsToLibrary(documentIds),
    onSuccess: async (result) => {
      if (result.ok) await loadLinkedDocuments(conversationId);
    },
  });
  const ingestWebSource = useMutation({
    mutationFn: ({ url, spaceId }: IngestWebSourceArgs) =>
      api.ingestWebUrl(url, { conversationId, spaceId }),
    onSuccess: async (result, { skipRefresh }) => {
      if (result.ok && !skipRefresh) await loadLinkedDocuments(conversationId);
    },
  });
  return {
    addDocumentsToLibrary: addToLibrary.mutateAsync,
    ingestWebSource: ingestWebSource.mutateAsync,
  };
}
