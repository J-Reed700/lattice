/**
 * File System Watch API Types
 *
 * Type definitions for file system watching and directory indexing.
 * These types match the Rust backend structures from commands/config.rs
 */

/**
 * Request to add a directory to the watch list.
 */
export interface AddWatchFolderRequest {
  /** Absolute directory path to watch */
  path: string;
}

/**
 * Request to remove a directory from the watch list.
 */
export interface RemoveWatchFolderRequest {
  /** Directory path to remove from watch list */
  path: string;
}

/**
 * Response from getting watched folders.
 * Returns array of absolute directory paths being watched for changes.
 */
export type WatchFoldersResponse = string[];

/**
 * Response from adding a watch folder.
 * Success is indicated by no error being thrown.
 */
export type AddWatchFolderResponse = void;

/**
 * Response from removing a watch folder.
 * Success is indicated by no error being thrown.
 */
export type RemoveWatchFolderResponse = void;

/**
 * Watch folder status with metadata.
 */
export interface WatchFolderStatus {
  /** Directory path being watched */
  path: string;
  /** Whether the directory currently exists */
  exists: boolean;
  /** Whether the directory is being actively watched */
  isWatching: boolean;
  /** Number of files in the directory (if available) */
  fileCount?: number;
  /** Last modification timestamp (ISO 8601, if available) */
  lastModified?: string;
}

/**
 * Helper to check if a specific path is being watched.
 */
export function isPathWatched(path: string, watchedFolders: string[]): boolean {
  return watchedFolders.includes(path);
}

/**
 * Helper to check if a path is under any watched folder.
 */
export function isPathUnderWatchedFolder(
  path: string,
  watchedFolders: string[]
): boolean {
  return watchedFolders.some((folder) => path.startsWith(folder));
}
