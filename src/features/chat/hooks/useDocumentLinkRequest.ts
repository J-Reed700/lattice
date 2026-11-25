import { useMutation, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { LIBRARY_DOCUMENTS_QUERY_KEY } from '@/features/files/hooks/useLibraryDocumentsQuery';
import { VaultAPI, type ApiClient } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

/**
 * Adds a document to a space, the one scoping lever the backend enforces.
 * Answers with the command's result so the caller can say why it failed.
 */
export function useLinkDocumentToSpaceMutation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ documentId, spaceId }: { documentId: string; spaceId: string }) =>
      api.setDocumentSpaceMembership(documentId, spaceId, true),
    onSuccess: async (result, { documentId }) => {
      if (!result.ok) return;
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: conversationKeys.memberships(documentId) }),
        queryClient.invalidateQueries({ queryKey: conversationKeys.all }),
        queryClient.invalidateQueries({ queryKey: ['spaces', 'documents'] }),
        queryClient.invalidateQueries({ queryKey: ['dashboard'] }),
        queryClient.invalidateQueries({ queryKey: LIBRARY_DOCUMENTS_QUERY_KEY }),
      ]);
    },
  });
}

/** The document's file name, or null when it cannot be read. */
export async function fetchDocumentFileName(documentId: string, api: ApiClient = VaultAPI): Promise<string | null> {
  const document = await api.getDocument(documentId);
  return document.ok ? document.data.fileName : null;
}
