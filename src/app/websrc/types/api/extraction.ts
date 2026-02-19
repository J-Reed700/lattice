/**
 * Extraction API Types
 *
 * Type definitions for wikilink parsing and content extraction.
 * These types match the Rust backend structures from commands/extraction.rs
 */

export interface WikiLink {
  target: string;
  display_text: string | null;
  header: string | null;
  line_number: number;
  // context field removed - Rust WikiLinkDto doesn't have this field
}

export interface ParsedLinksResponse {
  links: WikiLink[];
  count: number;
}

export interface ResolveLinkResponse {
  resolved_path: string | null;
}

export interface ResolvedLink {
  target: string;
  displayText: string | null;
  header: string | null;
  lineNumber: number;
  context: string;
  resolvedPath: string | null;
}

export interface ExtractAndResolveLinksResponse {
  links: ResolvedLink[];
  count: number;
}

export interface DocumentMetadata {
  title?: string;
  file_type?: string;
  author?: string;
}

export interface GenerateTagsResponse {
  tags: string[];
  count: number;
}

export interface DocumentInput {
  content: string;
  metadata?: DocumentMetadata;
}

export interface TagBatchResult {
  index: number;
  tags: string[];
  count: number;
}

export interface GenerateTagsBatchResponse {
  results: TagBatchResult[];
  count: number;
}
