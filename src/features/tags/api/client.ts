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

  removeTagFromDocument: async (
    request: RemoveTagFromDocumentRequest,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('remove_tag_from_document', {
      request: {
        document_id: request.documentId,
        tag_id: request.tagId,
      },
    }),

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

  getAllTagsWithCounts: async (): Promise<ApiResult<Wire.TagWithCountDto[]>> =>
    apiCall<Wire.TagWithCountDto[]>('get_all_tags_with_counts'),

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
