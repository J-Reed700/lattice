import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  ParsedLinksResponse,
  ExtractAndResolveLinksResponse,
} from '@/types';

export const extractionApi = {
  parseWikilinks: async (
    text: string,
    sourcePath?: string,
  ): Promise<ApiResult<ParsedLinksResponse>> =>
    apiCall<Wire.ParsedLinksResponse>('parse_wikilinks', { text, sourcePath }),

  extractDocumentTitle: async (
    content: string,
  ): Promise<ApiResult<string | null>> =>
    apiCall<string | null>('extract_document_title', { content }),

  resolveWikilink: async (
    target: string,
    sourcePath: string,
    availableDocuments: Wire.DocumentRefDto[],
  ): Promise<ApiResult<Wire.ResolveLinkResponse>> =>
    apiCall<Wire.ResolveLinkResponse>('resolve_wikilink', {
      target,
      sourcePath,
      availableDocuments,
    }),

  extractAndResolveLinks: async (
    content: string,
    documentId: string,
  ): Promise<ApiResult<ExtractAndResolveLinksResponse>> =>
    apiCall<Wire.ExtractAndResolveResponseDto>('extract_and_resolve_links', {
      content,
      documentId,
    }),
};
