import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  WebIngestResponse,
  UrlPreview,
  CleanArticle,
  WebPage,
} from '@/types';

export const webApi = {
  fetchUrlPreview: async (url: string): Promise<ApiResult<UrlPreview>> =>
    apiCall<Wire.UrlPreview>('fetch_url_preview', { url }),

  extractArticle: async (url: string): Promise<ApiResult<CleanArticle>> =>
    apiCall<Wire.CleanArticle>('extract_article', { url }),

  ingestWebUrl: async (
    url: string,
    options?: { spaceId?: string; conversationId?: string },
  ): Promise<ApiResult<WebIngestResponse>> =>
    apiCall<Wire.WebIngestResponse>('ingest_web_url', {
      url,
      spaceId: options?.spaceId,
      conversationId: options?.conversationId,
    }),

  /** Reads article text of a cited web page from cache, falling back to a fresh fetch. */
  readWebPage: async (url: string): Promise<ApiResult<WebPage>> =>
    apiCall<WebPage>('read_web_page', { url }),
};
