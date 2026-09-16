/**
 * File Management API Types
 *
 * Type definitions for file operations and metadata.
 * These types match the Rust backend structures from commands/file.rs
 */

export interface FileMetadata {
  fileName: string;
  mimeType: string;
  sizeBytes: number;      // Matches Rust size_bytes
  modifiedAt: string;     // Matches Rust modified_at
  isReadable: boolean;
  isWritable: boolean;
  path: string;
}

export interface IndexedFolder {
  path: string;
  recursive: boolean;
  enabled: boolean;
  lastScan: string | null;
  documentCount: number;
  createdAt: string;
}

export interface IndexingActivity {
  id: string;
  action: string;
  file_path: string;
  status: string;
  timestamp: string;
  details: string | null;
}

export type FileType =
  | 'web_article_html'
  | 'pdf'
  | 'image'
  | 'text'
  | 'unknown';

export interface OpenFileResponseDto {
  action: 'render_internal' | 'opened_external';
  fileType: FileType;
  contentPath: string;
  title?: string;
}

export interface IndexFileResponse {
  documentId: string;
  chunksCreated: number;
  status: 'indexed' | 'already_indexed' | 'updated' | 'imported';
  error: string | null;
  filePath: string;
}
