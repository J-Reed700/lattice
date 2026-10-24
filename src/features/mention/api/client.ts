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
  extractMentions: async (
    documentId: string,
    content: string,
  ): Promise<ApiResult<ExtractMentionsResponse>> =>
    apiCall<Wire.ExtractMentionsResultDto>('extract_mentions', {
      documentId,
      content,
    }),

  searchMentions: async (
    query: string,
    limit?: number,
  ): Promise<ApiResult<SearchMentionsResponse>> =>
    apiCall<Wire.SearchMentionsResultDto>('search_mentions', { query, limit }),

  getMentionsForDocument: async (
    documentId: string,
  ): Promise<ApiResult<GetMentionsForDocumentResult>> =>
    apiCall<Wire.GetMentionsForDocumentResultDto>('get_mentions_for_document', {
      documentId,
    }),

  getBacklinksForMention: async (
    mentionName: string,
  ): Promise<ApiResult<BacklinksResponse>> =>
    apiCall<Wire.BacklinksResultDto>('get_backlinks_for_mention', {
      mentionName,
    }),

  getMentionsByType: async (
    mentionType: string,
  ): Promise<ApiResult<Mention[]>> =>
    apiCall<Wire.MentionDto[]>('get_mentions_by_type', { mentionType }),

  createMention: async (
    name: string,
    mentionType: string,
  ): Promise<ApiResult<Mention>> =>
    apiCall<Wire.MentionDto>('create_mention', { name, mentionType }),

  deleteMention: async (mentionId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_mention', { mentionId }),
};
