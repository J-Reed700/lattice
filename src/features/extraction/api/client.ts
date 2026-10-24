import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  ParsedLinksResponse,
  ExtractAndResolveLinksResponse,
} from '@/types';

export const extractionApi = {
  /**
   * Parses wikilinks from markdown text.
   * Extracts [[wikilink]] syntax and returns structured link objects.
   *
   * @param text - Markdown text to parse
   * @param sourcePath - Source file path for relative link resolution (optional)
   * @returns Array of parsed wikilink objects
   */
  parseWikilinks: async (
    text: string,
    sourcePath?: string,
  ): Promise<ApiResult<ParsedLinksResponse>> =>
    apiCall<Wire.ParsedLinksResponse>('parse_wikilinks', { text, sourcePath }),

  /**
   * Extracts the document title from content.
   * Looks for first H1 heading or YAML frontmatter title.
   *
   * @param content - Document content (markdown)
   * @returns Extracted title string or null if not found
   */
  extractDocumentTitle: async (
    content: string,
  ): Promise<ApiResult<string | null>> =>
    apiCall<string | null>('extract_document_title', { content }),

  /**
   * Resolves a wikilink target to an absolute file path.
   * Handles relative paths, aliases, and folder-relative links.
   *
   * @param target - Wikilink target (e.g., 'Page Name' or '../folder/page')
   * @param sourcePath - Source file path for relative resolution
   * @param availableDocuments - Candidate documents to resolve against
   * @returns Resolution and matching document, if found
   */
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

  /**
   * Extracts and resolves all links from document content.
   * Combines parsing and resolution into single operation.
   *
   * @param content - Document content to parse
   * @param sourcePath - Source file path for resolution
   * @returns Object with extracted and resolved links
   */
  extractAndResolveLinks: async (
    content: string,
    documentId: string,
  ): Promise<ApiResult<ExtractAndResolveLinksResponse>> =>
    apiCall<Wire.ExtractAndResolveResponseDto>('extract_and_resolve_links', {
      content,
      documentId,
    }),
};
