import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';

export const explorerApi = {
  explorerResolveRoot: (
    path: string,
  ): Promise<ApiResult<Wire.ExplorerRootDto>> =>
    apiCall('explorer_resolve_root', { path }),

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

  setConversationExplorerRoot: (
    conversationId: string,
    root: string | null,
  ): Promise<ApiResult<void>> =>
    apiCall('set_conversation_explorer_root', { conversationId, root }),

  explorerIndexOpen: (
    root: string,
  ): Promise<ApiResult<Wire.FolderIndexStatusDto>> =>
    apiCall('explorer_index_open', { root }),

  explorerIndexClose: (): Promise<ApiResult<void>> =>
    apiCall('explorer_index_close'),

  explorerIndexStatus: (
    root: string,
  ): Promise<ApiResult<Wire.FolderIndexStatusDto>> =>
    apiCall('explorer_index_status', { root }),

  explorerIndexRebuild: (
    root: string,
  ): Promise<ApiResult<Wire.FolderIndexStatusDto>> =>
    apiCall('explorer_index_rebuild', { root }),

  explorerFoldersList: (): Promise<ApiResult<Wire.ExplorerFolderListDto>> =>
    apiCall('explorer_folders_list'),

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

  explorerFolderSetLastThread: (
    root: string,
    conversationId: string,
  ): Promise<ApiResult<void>> =>
    apiCall('explorer_folder_set_last_thread', { root, conversationId }),

  explorerFolderSetSettings: (
    root: string,
    instructions: string,
    spaceId: string,
  ): Promise<ApiResult<number>> =>
    apiCall('explorer_folder_set_settings', { root, instructions, spaceId }),

  explorerFolderDeleteIndex: (root: string): Promise<ApiResult<void>> =>
    apiCall('explorer_folder_delete_index', { root }),

  explorerFolderRemove: (
    root: string,
    deleteThreads: boolean,
  ): Promise<ApiResult<number>> =>
    apiCall('explorer_folder_remove', { root, deleteThreads }),
};
