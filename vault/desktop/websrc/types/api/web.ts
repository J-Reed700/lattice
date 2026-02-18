/**
 * Web Content and Download API Types
 *
 * Types for web content ingestion, URL previews, and download operations.
 */

/**
 * Web content ingestion response
 */
export interface WebIngestResponse {
  /** Unique document ID */
  documentId: string;
  /** Final URL (after redirects) */
  url: string;
  /** Document title */
  title: string;
  /** Word count */
  wordCount: number;
  /** Number of chunks created */
  chunks: number;
  /** Site name (if available) */
  siteName?: string;
  /** Author (if available) */
  author?: string;
  /** Estimated reading time in minutes */
  readingTimeMinutes?: number;
}

/**
 * URL preview metadata
 */
export interface UrlPreview {
  /** Page URL */
  url: string;
  /** Page title */
  title: string;
  /** Page description */
  description?: string;
  /** Preview image URL */
  image?: string;
  /** Site name */
  siteName?: string;
  /** Author name */
  author?: string;
  /** Publication timestamp (camelCase from backend) */
  publishedDate?: string;
  /** Legacy alias (avoid using in new code) */
  publishedTime?: string;
  /** Word count */
  wordCount: number;
  /** Estimated reading time in minutes */
  readingTimeMinutes: number;
  /** Language code (e.g., "en") */
  language?: string;
  /** Content type (e.g., "article") */
  contentType?: string;
  /** Extracted keywords */
  keywords?: string[];
}

/**
 * Extracted article content
 */
export interface CleanArticle {
  /** Article title */
  title: string;
  /** Clean HTML content */
  content: string;
  /** Plain text content */
  textContent: string;
  /** Article summary/excerpt */
  excerpt?: string;
  /** Author name */
  author?: string;
  /** Word count */
  wordCount: number;
  /** Estimated reading time in minutes */
  readingTimeMinutes: number;
  /** Publication timestamp (camelCase from backend) */
  publishedDate?: string;
  /** Legacy alias (avoid using in new code) */
  publishedTime?: string;
}

/**
 * Download status information
 */
export interface DownloadStatus {
  /** Download session ID */
  id: string;
  /** Download URL */
  url: string;
  /** Destination file path */
  destination: string;
  /** Current download state */
  state: string;
  /** Bytes downloaded so far */
  bytesDownloaded: number;
  /** Total file size in bytes (if known) */
  totalBytes: number | null;
  /** Download speed in bytes/second */
  bytesPerSecond: number;
  /** Download progress percentage (0-100) */
  percentage: number | null;
  /** Estimated time remaining in seconds */
  etaSeconds: number | null;
  /** Error message if failed */
  errorMessage: string | null;
  /** Number of retry attempts */
  retryCount: number;
  /** Creation timestamp (ISO 8601) */
  createdAt: string;
  /** Start timestamp (ISO 8601) */
  startedAt: string | null;
  /** Completion timestamp (ISO 8601) */
  completedAt: string | null;
  /** Associated model name */
  modelName?: string;
  /** Associated model ID */
  modelId?: string;
}

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
