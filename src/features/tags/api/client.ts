import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  Tag,
  ListTagsResponse,
  RemoveTagFromDocumentRequest,
  GetDocumentTagsRequest,
  DocumentTagsResponse,
} from '@/types';

export const tagsApi = {
  /**
   * Retrieves all tags in the system.
   * Returns tags with document counts and metadata.
   *
   * @returns Array of all tags with full metadata
   *
   * @example
   * const result = await VaultAPI.listAllTags();
   * if (result.ok) {
   *   console.log(`Found ${result.data.tags.length} tags`);
   * }
   */
  listAllTags: async (): Promise<ApiResult<ListTagsResponse>> => {
    const result = await apiCall<Wire.TagWithCountDto[]>(
      'get_all_tags_with_counts',
    );
    if (!result.ok) return result;
    return {
      ok: true,
      data: { tags: result.data.map((tag) => ({ ...tag, description: null })) },
    };
  },

  /**
   * Removes a tag assignment from a document.
   * Tag itself is not deleted, only the association.
   * Idempotent operation.
   *
   * @param request - Remove tag request with document and tag IDs
   * @returns Void on success
   *
   * @example
   * const result = await VaultAPI.removeTagFromDocument({
   *   documentId: 'doc_123',
   *   tagId: 'tag_456'
   * });
   */
  removeTagFromDocument: async (
    request: RemoveTagFromDocumentRequest,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('remove_tag_from_document', {
      request: {
        document_id: request.documentId,
        tag_id: request.tagId,
      },
    }),

  /**
   * Retrieves all tags applied to a specific document.
   *
   * @param request - Request with document ID
   * @returns Document tags response with tag list
   *
   * @example
   * const result = await VaultAPI.getDocumentTags({ documentId: 'doc_123' });
   * if (result.ok) {
   *   console.log(`Document has ${result.data.tags.length} tags`);
   * }
   */
  getDocumentTags: async (
    request: GetDocumentTagsRequest,
  ): Promise<ApiResult<DocumentTagsResponse>> => {
    const result = await apiCall<Wire.TagDto[]>('get_document_tags', {
      documentId: request.documentId,
    });
    if (!result.ok) return result;
    return {
      ok: true,
      data: { documentId: request.documentId, tags: result.data },
    };
  },

  /**
   * Retrieves all tags with document counts.
   * Useful for tag clouds and filter UIs that show tag popularity.
   * Tags are returned sorted by document count (descending).
   *
   * @returns Array of tags with document counts
   *
   * @example
   * const result = await VaultAPI.getAllTagsWithCounts();
   * if (result.ok) {
   *   result.data.forEach(tag => {
   *     console.log(`${tag.name}: ${tag.document_count} documents`);
   *   });
   * } else {
   *   console.error('Failed to load tags:', result.error);
   * }
   */
  getAllTagsWithCounts: async (): Promise<ApiResult<Wire.TagWithCountDto[]>> =>
    apiCall<Wire.TagWithCountDto[]>('get_all_tags_with_counts'),

  /**
   * Applies multiple tags to a document by name.
   * Creates tags if they don't exist. Returns updated tag list.
   * This is a convenience method that handles both tag creation and assignment.
   *
   * @param documentId - Document to tag
   * @param tagNames - Array of tag names to apply (case-insensitive)
   * @returns Array of applied tags with full metadata
   *
   * @example
   * const result = await VaultAPI.applyTags('doc_123', ['research', 'ai', 'important']);
   * if (result.ok) {
   *   console.log(`Applied ${result.data.length} tags`);
   *   result.data.forEach(tag => console.log(`- ${tag.name}`));
   * } else {
   *   console.error('Failed to apply tags:', result.error);
   * }
   */
  applyTags: async (
    documentId: string,
    tagNames: string[],
  ): Promise<ApiResult<Tag[]>> =>
    apiCall<Wire.TagDto[]>('apply_tags', {
      request: {
        document_id: documentId,
        tag_names: tagNames,
      },
    }),

  /**
   * Generates tags for a document using LLM analysis.
   * Backend fetches document content and analyzes it.
   * This is a convenience wrapper for document-based tag generation.
   *
   * @param documentId - Document to generate tags for
   * @returns Array of suggested tag names
   *
   * @example
   * const result = await VaultAPI.generateTagsForDocument('doc_123');
   * if (result.ok) {
   *   console.log('Suggested tags:', result.data);
   *   await VaultAPI.applyTags('doc_123', result.data);
   * }
   */
  generateTagsForDocument: async (
    documentId: string,
  ): Promise<ApiResult<string[]>> =>
    apiCall<string[]>('generate_tags_for_document', {
      request: {
        document_id: documentId,
        max_tags: 5,
      },
    }),
};
