import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';

export const explorerApi = {
  /** Canonicalises a folder the user picked; it must be an existing directory. */
  explorerResolveRoot: (
    path: string,
  ): Promise<ApiResult<Wire.ExplorerRootDto>> =>
    apiCall('explorer_resolve_root', { path }),

  /** One level of a directory under `root`; `path` is relative, `""` is the root. */
  explorerListDir: (
    root: string,
    path: string,
  ): Promise<ApiResult<Wire.ExplorerListingDto>> =>
    apiCall('explorer_list_dir', { root, path }),

  explorerReadFile: (
    root: string,
    path: string,
  ): Promise<ApiResult<Wire.ExplorerFileDto>> =>
    apiCall('explorer_read_file', { root, path }),

  /** Files a path from an answer most likely means: itself, else by path ending, else by name. */
  explorerLocateFile: (
    root: string,
    path: string,
  ): Promise<ApiResult<string[]>> =>
    apiCall('explorer_locate_file', { root, path }),

  explorerSearch: (
    root: string,
    query: string,
    options: {
      regex?: boolean;
      pathPrefix?: string | null;
      maxResults?: number | null;
    } = {},
  ): Promise<ApiResult<Wire.ExplorerSearchResultDto>> =>
    apiCall('explorer_search', {
      root,
      query,
      regex: options.regex ?? false,
      pathPrefix: options.pathPrefix ?? null,
      maxResults: options.maxResults ?? null,
    }),

  /** Binds a conversation to a folder; `null` unbinds it. */
  setConversationExplorerRoot: (
    conversationId: string,
    root: string | null,
  ): Promise<ApiResult<void>> =>
    apiCall('set_conversation_explorer_root', { conversationId, root }),

  /** Opens the folder's search index and starts or resumes indexing; closes any other. */
  explorerIndexOpen: (
    root: string,
  ): Promise<ApiResult<Wire.FolderIndexStatusDto>> =>
    apiCall('explorer_index_open', { root }),

  /** Closes the open folder's index (watcher, indexing, database). */
  explorerIndexClose: (): Promise<ApiResult<void>> =>
    apiCall('explorer_index_close'),

  explorerIndexStatus: (
    root: string,
  ): Promise<ApiResult<Wire.FolderIndexStatusDto>> =>
    apiCall('explorer_index_status', { root }),

  /** Wipes the folder's index and builds it again. */
  explorerIndexRebuild: (
    root: string,
  ): Promise<ApiResult<Wire.FolderIndexStatusDto>> =>
    apiCall('explorer_index_rebuild', { root }),

  /** The folders picked in the Explorer, pinned first, each with its threads and index. */
  explorerFoldersList: (): Promise<ApiResult<Wire.ExplorerFolderListDto>> =>
    apiCall('explorer_folders_list'),

  /** Renames a folder in the list; an empty name goes back to the folder's own. */
  explorerFolderRename: (
    root: string,
    name: string,
  ): Promise<ApiResult<void>> =>
    apiCall('explorer_folder_rename', { root, name }),

  explorerFolderSetPinned: (
    root: string,
    pinned: boolean,
  ): Promise<ApiResult<void>> =>
    apiCall('explorer_folder_set_pinned', { root, pinned }),

  /** Remembers the thread the folder's chat shows, so the folder reopens on it. */
  explorerFolderSetLastThread: (
    root: string,
    conversationId: string,
  ): Promise<ApiResult<void>> =>
    apiCall('explorer_folder_set_last_thread', { root, conversationId }),

  /**
   * Sets a folder's system prompt (empty for none) and the space its threads
   * belong to. Its threads move to that space; returns how many moved.
   */
  explorerFolderSetSettings: (
    root: string,
    instructions: string,
    spaceId: string,
  ): Promise<ApiResult<number>> =>
    apiCall('explorer_folder_set_settings', { root, instructions, spaceId }),

  /** Deletes the folder's own index and keeps it listed; the next open builds it again. */
  explorerFolderDeleteIndex: (root: string): Promise<ApiResult<void>> =>
    apiCall('explorer_folder_delete_index', { root }),

  /** Removes a folder and its own index from the list; with `deleteThreads`, its threads too. */
  explorerFolderRemove: (
    root: string,
    deleteThreads: boolean,
  ): Promise<ApiResult<number>> =>
    apiCall('explorer_folder_remove', { root, deleteThreads }),
};
