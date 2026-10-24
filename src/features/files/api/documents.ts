import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  IndexStatus,
  IndexingStats,
  IndexedFolder,
  IndexingActivity,
  IndexFileResponse,
  RecentDocument,
  DocumentMetadata,
  ApiResult,
  FileMetadata,
  OpenFileResponseDto,
  IndexingSnapshot,
  CorpusShapeDto,
  CitingConversationDto,
} from '@/types';

type BackendIndexProgress = Wire.IndexProgress;

function normalizeIndexingStats(raw: IndexingStats): IndexingStats {
  const indexedDocuments =
    raw.indexedDocuments ??
    (raw as { indexed_documents?: number }).indexed_documents ??
    0;
  const totalChunks =
    raw.totalChunks ?? (raw as { total_chunks?: number }).total_chunks ?? 0;

  return {
    indexedDocuments,
    totalChunks,
  };
}

function normalizeIndexProgress(raw: BackendIndexProgress): IndexingSnapshot {
  const totalFiles = raw.total_files ?? 0;
  const processed = raw.files_processed ?? 0;
  const failed = raw.failed ?? 0;
  const percentage =
    raw.percent_complete ??
    (totalFiles > 0 ? (processed / totalFiles) * 100 : 0);

  const validStatuses: ReadonlySet<string> = new Set([
    'idle',
    'scanning',
    'processing',
    'complete',
    'error',
    'cancelled',
  ]);
  if (!validStatuses.has(raw.status)) {
    throw new Error(`Unknown indexing status: ${raw.status}`);
  }
  const status = raw.status as IndexStatus;

  return {
    totalFiles,
    processed,
    failed,
    currentFile: raw.current_file ?? undefined,
    status,
    percentage,
    paused: raw.paused ?? false,
    failures: raw.failures ?? [],
  };
}

export const documentApi = {
  startIndexing: async (
    path: string,
    recursive?: boolean,
    spaceId?: string,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('index_directory', {
      path,
      recursive: recursive ?? true,
      spaceId,
    }),

  indexFile: async (
    path: string,
    spaceId?: string,
  ): Promise<ApiResult<IndexFileResponse>> =>
    apiCall<Wire.IndexFileResponseDto>('index_file', { path, spaceId }),

  reindexFile: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('reindex_file', { path }),

  removeIndexedFile: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_indexed_file', { path }),

  deleteDocument: async (documentId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_document', { documentId }),

  renameDocument: async (
    documentId: string,
    newName: string,
  ): Promise<ApiResult<Wire.RenameDocumentResponseDto>> =>
    apiCall<Wire.RenameDocumentResponseDto>('rename_document', {
      documentId,
      newName,
    }),

  getIndexProgress: async (): Promise<ApiResult<IndexingSnapshot>> => {
    const result = await apiCall<Wire.IndexProgress>('get_index_progress');
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: normalizeIndexProgress(result.data) };
  },

  cancelIndexing: async (): Promise<ApiResult<void>> =>
    apiCall<void>('cancel_indexing'),

  pauseIndexing: async (): Promise<ApiResult<void>> =>
    apiCall<void>('pause_indexing'),

  resumeIndexing: async (): Promise<ApiResult<void>> =>
    apiCall<void>('resume_indexing'),

  clearIndexingFailure: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('clear_indexing_failure', { path }),

  getIndexingStatus: async (): Promise<
    ApiResult<{ active: boolean; progress: number }>
  > => apiCall<Wire.IndexingStatus>('get_indexing_status'),

  listIndexedFiles: async (limit?: number): Promise<ApiResult<string[]>> =>
    apiCall<string[]>('list_indexed_files', { limit }),

  listCustomCollections: async (): Promise<
    ApiResult<Wire.CustomCollectionDto[]>
  > => apiCall<Wire.CustomCollectionDto[]>('list_custom_collections'),

  createCustomCollection: async (
    request: Wire.CreateCustomCollectionRequest,
  ): Promise<ApiResult<string>> =>
    apiCall<string>('create_custom_collection', { request }),

  renameCustomCollection: async (
    collectionId: string,
    name: string,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('rename_custom_collection', { collectionId, name }),

  moveCustomCollection: async (
    collectionId: string,
    parentId: string | null,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('move_custom_collection', { collectionId, parentId }),

  deleteCustomCollection: async (
    collectionId: string,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('delete_custom_collection', { collectionId }),

  addDocumentsToCustomCollection: async (
    collectionId: string,
    documentIds: string[],
  ): Promise<ApiResult<void>> =>
    apiCall<void>('add_documents_to_custom_collection', {
      collectionId,
      documentIds,
    }),

  removeDocumentsFromCustomCollection: async (
    collectionId: string,
    documentIds: string[],
  ): Promise<ApiResult<void>> =>
    apiCall<void>('remove_documents_from_custom_collection', {
      collectionId,
      documentIds,
    }),

  getIndexingStats: async (): Promise<ApiResult<IndexingStats>> => {
    const result = await apiCall<Wire.IndexingStatsDto>('get_indexing_stats');
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: normalizeIndexingStats(result.data) };
  },

  getIndexedFolders: async (): Promise<ApiResult<IndexedFolder[]>> =>
    apiCall<Wire.IndexedFolder[]>('get_indexed_folders'),

  getIndexingActivities: async (
    limit: number,
  ): Promise<ApiResult<IndexingActivity[]>> =>
    apiCall<Wire.IndexingActivity[]>('get_indexing_activities', { limit }),

  openFile: async (path: string): Promise<ApiResult<OpenFileResponseDto>> =>
    apiCall<OpenFileResponseDto>('open_file', { path }),

  openFileById: async (
    fileId: string,
  ): Promise<ApiResult<OpenFileResponseDto>> =>
    apiCall<OpenFileResponseDto>('open_file_by_id', { fileId }),

  getFilePathById: async (fileId: string): Promise<ApiResult<string>> =>
    apiCall<string>('get_file_path_by_id', { fileId }),

  getRecentDocuments: async (
    limit: number,
  ): Promise<ApiResult<RecentDocument[]>> =>
    apiCall<Wire.RecentDocument[]>('get_recent_documents', { limit }),

  listAllDocuments: async (
    limit: number,
  ): Promise<ApiResult<DocumentMetadata[]>> =>
    apiCall<Wire.DocumentMetadataDto[]>('list_all_documents', { limit }),

  getDocument: async (
    documentId: string,
  ): Promise<ApiResult<DocumentMetadata>> =>
    apiCall<DocumentMetadata>('get_document', { documentId }),

  readFileContent: async (path: string): Promise<ApiResult<string>> =>
    apiCall<string>('read_file_content', { path }),

  readFileBytes: async (path: string): Promise<ApiResult<Uint8Array>> => {
    const result = await apiCall<number[]>('read_file_bytes', { path });
    if (!result.ok) return result;
    return { ok: true, data: Uint8Array.from(result.data) };
  },

  getFileMetadata: async (path: string): Promise<ApiResult<FileMetadata>> =>
    apiCall<Wire.FileMetadataDto>('get_file_metadata', { path }),

  showInFolder: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('show_in_folder', { path }),

  getCorpusShape: async (): Promise<ApiResult<CorpusShapeDto>> =>
    apiCall<Wire.CorpusShapeDto>('get_corpus_shape'),

  listConversationsCitingDocument: async (
    documentId: string,
    limit?: number,
  ): Promise<ApiResult<CitingConversationDto[]>> =>
    apiCall<Wire.CitingConversationDto[]>(
      'list_conversations_citing_document',
      {
        documentId,
        limit,
      },
    ),
};
