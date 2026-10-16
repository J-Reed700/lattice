import { useMutation } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';

/**
 * Adds a document to a space, the one scoping lever the backend enforces.
 * Answers with the command's result so the caller can say why it failed.
 */
export function useLinkDocumentToSpaceMutation() {
  return useMutation({
    mutationFn: ({ documentId, spaceId }: { documentId: string; spaceId: string }) =>
      VaultAPI.setDocumentSpaceMembership(documentId, spaceId, true),
  });
}

/** The document's file name, or null when it cannot be read. */
export async function fetchDocumentFileName(documentId: string): Promise<string | null> {
  const document = await VaultAPI.getDocument(documentId);
  return document.ok ? document.data.fileName : null;
}
