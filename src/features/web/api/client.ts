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
  /**
   * Fetches preview metadata for a web URL without ingesting it.
   * Useful for validating and showing a preview before import.
   *
   * @param url - Web URL to preview (must be valid HTTP/HTTPS)
   * @returns UrlPreview metadata
   */
  fetchUrlPreview: async (url: string): Promise<ApiResult<UrlPreview>> =>
    apiCall<Wire.UrlPreview>('fetch_url_preview', { url }),

  /**
   * Extracts article content (reader mode) from a URL.
   * Returns clean text/HTML without ingesting.
   *
   * @param url - Web URL to extract
   * @returns CleanArticle content
   */
  extractArticle: async (url: string): Promise<ApiResult<CleanArticle>> =>
    apiCall<Wire.CleanArticle>('extract_article', { url }),

  /**
   * Ingests content from a web URL and adds it to the index.
   * Fetches page, extracts text, generates embeddings, and stores as document.
   *
   * @param url - Web URL to ingest (must be valid HTTP/HTTPS)
   * @returns WebIngestResponse with document ID and metadata
   */
  ingestWebUrl: async (
    url: string,
    options?: { spaceId?: string; conversationId?: string },
  ): Promise<ApiResult<WebIngestResponse>> =>
    apiCall<Wire.WebIngestResponse>('ingest_web_url', {
      url,

      spaceId: options?.spaceId,

      conversationId: options?.conversationId,
    }),

  /**
   * The article text of a web page a turn cited, from the page cache.
   *
   * Falls back to one validated fetch when the cache no longer holds it.
   */
  readWebPage: async (url: string): Promise<ApiResult<WebPage>> =>
    apiCall<WebPage>('read_web_page', { url }),
};
