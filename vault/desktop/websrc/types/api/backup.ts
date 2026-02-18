/**
 * Backup and Export API Types
 *
 * Types for backup creation, restore, and data export operations.
 */

/**
 * Backup file metadata
 */
export interface BackupInfo {
  /** Full path to backup file */
  path: string;
  /** Backup filename */
  name: string;
  /** ISO 8601 timestamp of backup creation */
  createdAt: string;
  /** Backup format version */
  version: string;
  /** Number of files in backup (reserved, currently 0) */
  fileCount: number;
  /** Backup file size in bytes */
  size: number;
}

/**
 * Export operation result
 */
export interface ExportResult {
  /** Number of files successfully exported */
  filesExported: number;
  /** Output directory or file path */
  outputPath: string;
  /** Export format used */
  format: 'markdown' | 'json' | 'csv' | 'html';
}
