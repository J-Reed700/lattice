export type WebIngestResponse = import('../../lib/bindings').WebIngestResponse;
export type UrlPreview = import('../../lib/bindings').UrlPreview;
export type CleanArticle = import('../../lib/bindings').CleanArticle;

export interface BatchDownloadResult {
  started: number;
  failed: number;
  downloadIds: string[];
}

export interface WebMetadata {
  url: string;
  title: string;
  description?: string;
  image?: string;
  siteName?: string;
  canonicalUrl?: string;
  language?: string;
  author?: string;
  publishedDate?: string;
  modifiedDate?: string;
}

export interface ExtractedContent {
  content: string;
  title: string;
  excerpt?: string;
  author?: string;
  wordCount: number;
}
