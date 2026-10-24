import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  Mention,
  ExtractMentionsResponse,
  SearchMentionsResponse,
  BacklinksResponse,
  GetMentionsForDocumentResult,
} from '@/types';

export const mentionApi = {
  /**
   * Extracts mentions (wikilinks, @-mentions) from document content.
   * Parses syntax like [[page]] and @person to create bidirectional links.
   *
   * @param documentId - Internal document identifier
   * @param content - Document content to parse
   * @returns Array of extracted mention objects
   */
  extractMentions: async (
    documentId: string,
    content: string,
  ): Promise<ApiResult<ExtractMentionsResponse>> =>
    apiCall<Wire.ExtractMentionsResultDto>('extract_mentions', {
      documentId,
      content,
    }),

  /**
   * Searches for mentions matching a query.
   * Finds mentions by name or partial name match.
   *
   * @param query - Search query for mention names
   * @param limit - Maximum number of results to return (optional)
   * @returns Array of matching mention objects
   */
  searchMentions: async (
    query: string,
    limit?: number,
  ): Promise<ApiResult<SearchMentionsResponse>> =>
    apiCall<Wire.SearchMentionsResultDto>('search_mentions', { query, limit }),

  /**
   * Gets all mentions found in a specific document.
   * Returns outgoing links from this document to other entities.
   *
   * @param documentId - Internal document identifier
   * @returns Array of mention objects found in the document
   */
  getMentionsForDocument: async (
    documentId: string,
  ): Promise<ApiResult<GetMentionsForDocumentResult>> =>
    apiCall<Wire.GetMentionsForDocumentResultDto>('get_mentions_for_document', {
      documentId,
    }),

  /**
   * Gets backlinks for a mention (documents that reference it).
   * Returns all documents that mention this entity.
   *
   * @param mentionName - Name of the mention to find backlinks for
   * @returns Array of documents that reference this mention
   */
  getBacklinksForMention: async (
    mentionName: string,
  ): Promise<ApiResult<BacklinksResponse>> =>
    apiCall<Wire.BacklinksResultDto>('get_backlinks_for_mention', {
      mentionName,
    }),

  /**
   * Gets mentions filtered by type.
   * Types include 'wikilink', 'person', 'hashtag', etc.
   *
   * @param mentionType - Type of mentions to retrieve
   * @returns Array of mentions of the specified type
   */
  getMentionsByType: async (
    mentionType: string,
  ): Promise<ApiResult<Mention[]>> =>
    apiCall<Wire.MentionDto[]>('get_mentions_by_type', { mentionType }),

  /**
   * Creates a new mention entity.
   * Manually adds a mention that can be referenced in documents.
   *
   * @param name - Mention name/identifier
   * @param mentionType - Type of mention (e.g., 'person', 'concept')
   * @returns Created mention object
   */
  createMention: async (
    name: string,
    mentionType: string,
  ): Promise<ApiResult<Mention>> =>
    apiCall<Wire.MentionDto>('create_mention', { name, mentionType }),

  /**
   * Deletes a mention entity.
   * Removes mention and all its backlink relationships.
   *
   * @param mentionId - Internal mention identifier
   * @returns Void on success
   */
  deleteMention: async (mentionId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_mention', { mentionId }),
};
