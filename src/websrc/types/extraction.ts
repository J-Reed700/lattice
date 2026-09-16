/**
 * TypeScript types for extraction IPC commands (Rust implementation)
 *
 * These types correspond to the Rust extraction commands that use:
 * - Link Parser: Extract and resolve [[wikilinks]]
 * - Tag Generator: Generate tags with Anthropic Claude (with prompt caching)
 */


/**
 * A parsed [[wikilink]] from markdown.
 */
export interface WikiLink {
  /** File containing the link */
  sourcePath: string;

  /** Target note path/title (e.g., "todo" from [[todo]]) */
  target: string;

  /** Custom display text (e.g., "display" from [[target|display]]) */
  displayText?: string;

  /** Header anchor (e.g., "section" from [[target#section]]) */
  header?: string;

  /** Line number in source file */
  lineNumber: number;

  /** Surrounding text for preview */
  context: string;
}

/**
 * Response from parsing wikilinks.
 */
export interface ParsedLinksResponse {
  links: WikiLink[];
  count: number;
}

/**
 * Document information for link resolution.
 */
export interface DocumentInfo {
  /** Full path to the document file */
  filePath: string;

  /** Optional document title (extracted from H1 or frontmatter) */
  title?: string;
}

/**
 * Response from resolving a wikilink.
 */
export interface ResolveLinkResponse {
  /** Resolved file path, or null if not found */
  resolvedPath?: string;
}

/**
 * Resolved link with full details.
 */
export interface ResolvedLink {
  target: string;
  displayText?: string;
  header?: string;
  lineNumber: number;
  context: string;
  resolvedPath?: string;
}


/**
 * Document metadata for tag generation.
 */
export interface DocumentMetadata {
  title?: string;
  fileType?: string;
  author?: string;
}

/**
 * Input document for tag generation.
 */
export interface DocumentInput {
  content: string;
  metadata?: DocumentMetadata;
}

/**
 * Response from generating tags.
 */
export interface GenerateTagsResponse {
  tags: string[];
  count: number;
}

/**
 * Tag generation result for batch operation.
 */
export interface TagGenerationResult {
  index: number;
  tags: string[];
  count: number;
}

/**
 * Response from batch tag generation.
 */
export interface BatchTagsResponse {
  results: TagGenerationResult[];
  count: number;
}

/**
 * LLM cache statistics.
 */
export interface CacheStats {
  hits: number;
  misses: number;
  totalTokensSaved: number;
  hitRatePercent: number;
}


/**
 * Parameters for parse_wikilinks command.
 */
export interface ParseWikilinksParams {
  text: string;
  sourcePath?: string;
}

/**
 * Parameters for resolve_wikilink command.
 */
export interface ResolveWikilinkParams {
  target: string;
  sourcePath: string;
  availableDocuments: DocumentInfo[];
}

/**
 * Parameters for extract_and_resolve_links command.
 */
export interface ExtractAndResolveLinksParams {
  content: string;
  sourcePath: string;
  availableDocuments: DocumentInfo[];
}

/**
 * Parameters for generate_tags_llm command.
 */
export interface GenerateTagsParams {
  content: string;
  maxTags?: number;
  apiKey?: string;
  metadata?: DocumentMetadata;
}

/**
 * Parameters for generate_tags_batch command.
 */
export interface BatchGenerateTagsParams {
  documents: DocumentInput[];
  maxTags?: number;
  apiKey?: string;
}

/**
 * Parameters for get_llm_cache_stats command.
 */
export interface GetCacheStatsParams {
  apiKey?: string;
}
