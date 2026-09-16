/**
 * Extraction API Types
 *
 * Type definitions for wikilink parsing and content extraction.
 * These types match the Rust backend structures from commands/extraction.rs
 */

export type WikiLink = import('../../lib/bindings').WikiLinkDto;

export type ParsedLinksResponse = import('../../lib/bindings').ParsedLinksResponse;

export type ResolveLinkResponse = import('../../lib/bindings').ResolveLinkResponse;

export type ResolvedLink = import('../../lib/bindings').ResolvedLinkDto;

export type ExtractAndResolveLinksResponse = import('../../lib/bindings').ExtractAndResolveResponseDto;

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
