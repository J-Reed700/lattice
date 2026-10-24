/**
 * Web Content and Download API Types
 *
 * Types for web content ingestion, URL previews, and download operations.
 */

/**
 * Web content ingestion response
 */
export type WebIngestResponse = import('../../lib/bindings').WebIngestResponse;

/**
 * URL preview metadata
 */
export type UrlPreview = import('../../lib/bindings').UrlPreview;

/**
 * Extracted article content
 */
export type CleanArticle = import('../../lib/bindings').CleanArticle;

/**
 * Batch download result
 */
export interface BatchDownloadResult {
  /** Number of downloads started */
  started: number;
  /** Number of downloads failed to start */
  failed: number;
  /** Download session IDs */
  downloadIds: string[];
}

/**
 * Web page metadata
 */
export interface WebMetadata {
  /** Page URL */
  url: string;
  /** Page title */
  title: string;
  /** Meta description */
  description?: string;
  /** OpenGraph image */
  image?: string;
  /** Site name */
  siteName?: string;
  /** Canonical URL */
  canonicalUrl?: string;
  /** Page language */
  language?: string;
  /** Author information */
  author?: string;
  /** Publication date */
  publishedDate?: string;
  /** Modified date */
  modifiedDate?: string;
}

/**
 * Extracted content structure
 */
export interface ExtractedContent {
  /** Main content text */
  content: string;
  /** Content title */
  title: string;
  /** Content excerpt */
  excerpt?: string;
  /** Author information */
  author?: string;
  /** Word count */
  wordCount: number;
}
