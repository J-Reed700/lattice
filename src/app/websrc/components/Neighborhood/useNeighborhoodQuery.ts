import { useQuery } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { DocumentMetadata } from '@/types/fileBrowser';

export interface NeighborhoodDocument {
  documentId: string;
  title: string;
}

export interface NeighborhoodConversation {
  conversationId: string;
  title: string;
}

export interface NeighborhoodData {
  /** Authored links, incoming then outgoing, deduped. Never mixed with "Similar". */
  links: NeighborhoodDocument[];
  similar: NeighborhoodDocument[];
  citedIn: NeighborhoodConversation[];
}

const MAX_LINKS = 12;
const MAX_SIMILAR = 6;

export const stripExtension = (fileName: string): string =>
  fileName.replace(/\.[^./\\]+$/, '');

const titleOf = (doc: DocumentMetadata): string => stripExtension(doc.fileName);

/**
 * What sits next to a document: what links to it, what reads like it, and where
 * it has come up in conversation.
 *
 * Every call is individually tolerant — a section that fails is an empty list,
 * never an error state for the whole panel, because three lists are more useful
 * than one apology.
 */
export async function loadNeighborhood(
  documentId: string,
  documentTitle: string,
  documentsById: Map<string, DocumentMetadata>,
): Promise<NeighborhoodData> {
  const [mentions, backlinks, similar, citing] = await Promise.all([
    VaultAPI.getMentionsForDocument(documentId),
    VaultAPI.getBacklinksForMention(stripExtension(documentTitle)),
    VaultAPI.findSimilarDocuments(documentId, MAX_SIMILAR),
    VaultAPI.listConversationsCitingDocument(documentId),
  ]);

  // Title index for resolving outgoing mention names back to documents.
  const byTitle = new Map<string, DocumentMetadata>();
  for (const doc of documentsById.values()) {
    byTitle.set(titleOf(doc).toLowerCase(), doc);
  }

  const links = new Map<string, NeighborhoodDocument>();

  if (backlinks.ok) {
    for (const id of backlinks.data.documentIds) {
      const doc = documentsById.get(id);
      // A link that goes nowhere is worse than no link.
      if (!doc || doc.id === documentId) continue;
      links.set(doc.id, { documentId: doc.id, title: titleOf(doc) });
    }
  }

  if (mentions.ok) {
    for (const mention of mentions.data.mentions) {
      const doc = byTitle.get(mention.name.trim().toLowerCase());
      if (!doc || doc.id === documentId) continue;
      links.set(doc.id, { documentId: doc.id, title: titleOf(doc) });
    }
  }

  const linkList = [...links.values()]
    .sort((a, b) => a.title.localeCompare(b.title))
    .slice(0, MAX_LINKS);

  const similarList = similar.ok
    ? similar.data
        .filter((hit) => hit.documentId !== documentId)
        .slice(0, MAX_SIMILAR)
        .map((hit) => ({
          documentId: hit.documentId,
          title: stripExtension(hit.title),
        }))
    : [];

  const citedInList = citing.ok
    ? citing.data.map((row) => ({ conversationId: row.conversationId, title: row.title }))
    : [];

  return { links: linkList, similar: similarList, citedIn: citedInList };
}

export function useNeighborhoodQuery(
  documentId: string,
  documentTitle: string,
  documentsById: Map<string, DocumentMetadata>,
) {
  return useQuery<NeighborhoodData>({
    queryKey: ['neighborhood', documentId],
    queryFn: () => loadNeighborhood(documentId, documentTitle, documentsById),
    enabled: documentId !== '',
    staleTime: 60_000,
  });
}
