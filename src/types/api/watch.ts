export interface AddWatchFolderRequest {
  path: string;
}

export interface RemoveWatchFolderRequest {
  path: string;
}

export type WatchFoldersResponse = string[];
export type AddWatchFolderResponse = void;
export type RemoveWatchFolderResponse = void;

export interface WatchFolderStatus {
  path: string;
  exists: boolean;
  isWatching: boolean;
  fileCount?: number;
  lastModified?: string;
}

export function isPathWatched(path: string, watchedFolders: string[]): boolean {
  return watchedFolders.includes(path);
}

export function isPathUnderWatchedFolder(
  path: string,
  watchedFolders: string[]
): boolean {
  return watchedFolders.some((folder) => path.startsWith(folder));
}
