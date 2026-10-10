import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';

import { fetchMemberships } from '@/features/chat/api/conversationQueryData';
import { CORPUS_SHAPE_QUERY_KEY } from '@/features/corpus-shape/hooks/useCorpusShapeQuery';
import { CUSTOM_COLLECTIONS_QUERY_KEY } from '@/features/files/hooks/useCustomCollectionsQuery';
import { INDEXED_FOLDERS_QUERY_KEY } from '@/features/files/hooks/useIndexedFoldersQuery';
import { INDEXING_ACTIVITIES_QUERY_KEY, INDEXING_STATUS_QUERY_KEY } from '@/features/files/hooks/useIndexingStatusQuery';
import { LIBRARY_DOCUMENTS_QUERY_KEY } from '@/features/files/hooks/useLibraryDocumentsQuery';
import VaultAPI from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { unwrapApiResult } from '@/types/api/result';
import type { DocumentMetadata } from '@/types/fileBrowser';

import { THEMES_QUERY_KEY } from './useClustersQuery';

type LibraryDocument = Pick<DocumentMetadata, 'id' | 'filePath' | 'fileName'>;

const isHttpUrl = (path: string) => /^https?:\/\//i.test(path);
const isAbsolutePath = (path: string) => path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path);

/** Everything that lists or counts library documents. */
const DOCUMENT_VIEW_KEYS = [
  LIBRARY_DOCUMENTS_QUERY_KEY,
  CUSTOM_COLLECTIONS_QUERY_KEY,
  THEMES_QUERY_KEY,
  INDEXED_FOLDERS_QUERY_KEY,
  CORPUS_SHAPE_QUERY_KEY,
  ['dashboard'],
  ['chat-empty-state-stats'],
  ['learning-space-documents'],
  // Space memberships and the documents a conversation links.
  conversationKeys.all,
] as const;

/** A document came or went: every view of the library reads again. */
const refreshDocumentViews = (client: QueryClient) =>
  Promise.all(DOCUMENT_VIEW_KEYS.map((queryKey) => client.invalidateQueries({ queryKey })));

/** The spaces a document is filed in. */
export function useDocumentSpaceMembershipsQuery(documentId: string) {
  return useQuery({
    queryKey: conversationKeys.memberships(documentId),
    queryFn: () => fetchMemberships(documentId),
  });
}

/** An absolute local path for the document, asking the backend when the row only has an id; `null` for none. */
export async function resolveDocumentPath(doc: LibraryDocument): Promise<string | null> {
  if (!isHttpUrl(doc.filePath) && isAbsolutePath(doc.filePath)) return doc.filePath;
  const result = await VaultAPI.getFilePathById(doc.id);
  return result.ok ? result.data : null;
}

/** Opens the document in the system viewer, or says to render a web article in the app. */
export const openDocumentFile = (documentId: string) => VaultAPI.openFileById(documentId);

/** The document's file as text, for searching inside it. */
export const readDocumentText = (filePath: string) => VaultAPI.readFileContent(filePath);

/** Reveals the document's file in Finder or Explorer. */
export async function showDocumentInFolder(documentId: string): Promise<void> {
  const path = unwrapApiResult(await VaultAPI.getFilePathById(documentId));
  unwrapApiResult(await VaultAPI.showInFolder(path));
}

const pathFor = async (doc: LibraryDocument) => {
  const path = await resolveDocumentPath(doc);
  if (!path) throw new Error('No file path for this document.');
  return path;
};

export interface DeleteDocumentsRequest {
  ids: readonly string[];
  /** Asked before each delete; `true` stops the run there. */
  shouldStop?: () => boolean;
  /** Called after each attempt with how many have been tried. */
  onProgress?: (_attempted: number) => void;
}

export interface DeleteDocumentsOutcome {
  deleted: number;
  errors: string[];
}

/**
 * The library's writes on documents. Each refreshes what it changes:
 * deleting or unindexing a document refreshes every view of the library,
 * filing one in a space refreshes its memberships and the space's lists,
 * and reindexing refreshes the indexing status.
 */
export function useLibraryDocumentActions() {
  const client = useQueryClient();
  const reindex = useMutation({
    mutationFn: async (doc: LibraryDocument) => unwrapApiResult(await VaultAPI.reindexFile(await pathFor(doc))),
    onSuccess: () => Promise.all([
      client.invalidateQueries({ queryKey: INDEXING_STATUS_QUERY_KEY }),
      client.invalidateQueries({ queryKey: INDEXING_ACTIVITIES_QUERY_KEY }),
      client.invalidateQueries({ queryKey: LIBRARY_DOCUMENTS_QUERY_KEY }),
    ]),
  });
  const removeFromIndex = useMutation({
    mutationFn: async (doc: LibraryDocument) => unwrapApiResult(await VaultAPI.removeIndexedFile(await pathFor(doc))),
    onSuccess: () => Promise.all([
      refreshDocumentViews(client),
      client.invalidateQueries({ queryKey: INDEXING_STATUS_QUERY_KEY }),
    ]),
  });
  const setSpaceMembership = useMutation({
    mutationFn: async ({ documentId, spaceId, assigned }: { documentId: string; spaceId: string; assigned: boolean }) =>
      unwrapApiResult(await VaultAPI.setDocumentSpaceMembership(documentId, spaceId, assigned)),
    onSuccess: (_result, { documentId }) => Promise.all([
      client.invalidateQueries({ queryKey: conversationKeys.memberships(documentId) }),
      client.invalidateQueries({ queryKey: ['learning-space-documents'] }),
    ]),
  });
  const addToSpace = useMutation({
    mutationFn: async ({ documentIds, spaceId }: { documentIds: string[]; spaceId: string }) =>
      unwrapApiResult(await VaultAPI.setDocumentsSpaceMembership(documentIds, spaceId, true)),
    onSuccess: (_result, { documentIds }) => Promise.all([
      ...documentIds.map((documentId) => client.invalidateQueries({ queryKey: conversationKeys.memberships(documentId) })),
      client.invalidateQueries({ queryKey: ['learning-space-documents'] }),
    ]),
  });
  const deleteDocuments = useMutation({
    mutationFn: async ({ ids, shouldStop, onProgress }: DeleteDocumentsRequest): Promise<DeleteDocumentsOutcome> => {
      const outcome: DeleteDocumentsOutcome = { deleted: 0, errors: [] };
      for (const [index, id] of ids.entries()) {
        if (shouldStop?.()) break;
        const result = await VaultAPI.deleteDocument(id);
        if (result.ok) outcome.deleted += 1;
        else outcome.errors.push(result.error || 'Unknown error');
        onProgress?.(index + 1);
      }
      return outcome;
    },
    // A partial run still deleted something.
    onSettled: () => refreshDocumentViews(client),
  });

  return { reindex, removeFromIndex, setSpaceMembership, addToSpace, deleteDocuments };
}
