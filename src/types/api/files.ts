/**
 * File Management API Types
 *
 * Type definitions for file operations and metadata.
 * These types match the Rust backend structures from commands/file.rs
 */

export type FileMetadata = import('../../lib/bindings').FileMetadataDto;

export type IndexedFolder = import('../../lib/bindings').IndexedFolder;

export type IndexingActivity = import('../../lib/bindings').IndexingActivity;

export type FileType =
  | 'web_article_html'
  | 'pdf'
  | 'image'
  | 'text'
  | 'unknown';

export type OpenFileResponseDto = import('../../lib/bindings').OpenFileResponseDto;

export type IndexFileResponse = import('../../lib/bindings').IndexFileResponseDto;
