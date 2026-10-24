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
  /**
   * Starts indexing a folder and all its contents.
   * Extracts text, generates embeddings, and stores in database.
   * Supports recursive directory traversal.
   *
   * @param path - Absolute path to folder to index
   * @param recursive - Whether to index subdirectories (default: true)
   * @param spaceId - Optional space to assign indexed documents to
   * @returns Void on success
   */
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

  /**
   * Indexes a single file by extracting content and generating embeddings.
   * Supports markdown, text, PDF, DOCX, and other document formats.
   *
   * @param path - Absolute path to file to index
   * @param spaceId - Optional space to assign the indexed document to
   * @returns IndexFileResponse with status information
   */
  indexFile: async (
    path: string,
    spaceId?: string,
  ): Promise<ApiResult<IndexFileResponse>> =>
    apiCall<Wire.IndexFileResponseDto>('index_file', { path, spaceId }),

  /**
   * Reindexes an existing file with fresh content and embeddings.
   * Useful when file content has changed or to update metadata.
   *
   * @param path - Absolute path to file to reindex
   * @returns Void on success
   */
  reindexFile: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('reindex_file', { path }),

  /**
   * Removes a file from the search index and database.
   * Deletes document record, embeddings, and all associated metadata.
   *
   * @param path - Absolute path to file to remove
   * @returns Void on success
   */
  removeIndexedFile: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_indexed_file', { path }),

  /**
   * Deletes a document by ID from the search index and database.
   * Removes document record, all chunks, and vector embeddings.
   * This operation is irreversible.
   *
   * @param documentId - Unique identifier of the document to delete
   * @returns Void on success
   */
  deleteDocument: async (documentId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_document', { documentId }),

  /**
   * Renames a document's display name in the database.
   * Only updates the metadata - does NOT modify file content or regenerate embeddings.
   * This follows the user's requirement that modifying content would be too complicated.
   *
   * @param documentId - Unique identifier of the document to rename
   * @param newName - New display name (1-255 chars, no path separators)
   * @returns Response with confirmation and new name
   *
   * @example
   * const result = await VaultAPI.renameDocument('doc-123', 'New Name.pdf');
   * if (result.ok) {
   *   console.log(result.data.message); // "Document renamed from '...' to '...'"
   * }
   */
  renameDocument: async (
    documentId: string,
    newName: string,
  ): Promise<ApiResult<Wire.RenameDocumentResponseDto>> =>
    apiCall<Wire.RenameDocumentResponseDto>('rename_document', {
      documentId,
      newName,
    }),

  /**
   * Retrieves current indexing progress for active indexing operations.
   * Includes files processed, total files, current file, and completion percentage.
   *
   * @returns IndexProgress object with current state
   */
  getIndexProgress: async (): Promise<ApiResult<IndexingSnapshot>> => {
    const result = await apiCall<Wire.IndexProgress>('get_index_progress');
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: normalizeIndexProgress(result.data) };
  },

  /**
   * Cancels any currently running indexing operation.
   * Gracefully stops processing and preserves already-indexed files.
   *
   * @returns Void on success
   */
  cancelIndexing: async (): Promise<ApiResult<void>> =>
    apiCall<void>('cancel_indexing'),

  /** Pauses the running index. Both the directory run and the watcher queue stop. */
  pauseIndexing: async (): Promise<ApiResult<void>> =>
    apiCall<void>('pause_indexing'),

  /** Resumes a paused index. */
  resumeIndexing: async (): Promise<ApiResult<void>> =>
    apiCall<void>('resume_indexing'),

  /**
   * Drops one path from the run's failure list.
   *
   * The list lives in the indexing engine's state, which is what every view
   * reads through `getIndexProgress` — dismissing a row locally would let it
   * reappear the moment another surface reads the snapshot. Unknown paths are
   * a no-op on the backend.
   */
  clearIndexingFailure: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('clear_indexing_failure', { path }),

  /**
   * Thin `{ active, progress }` view of the index.
   * Prefer `getIndexProgress`, which carries the counts, the status word, the
   * pause flag and the failure list.
   */
  getIndexingStatus: async (): Promise<
    ApiResult<{ active: boolean; progress: number }>
  > => apiCall<Wire.IndexingStatus>('get_indexing_status'),

  /** Absolute paths of indexed files, newest first. */
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

  /**
   * Gets comprehensive statistics about indexed content.
   * Includes total documents, total size, file type breakdown, and more.
   *
   * @returns IndexingStats object with counts and metrics
   */
  getIndexingStats: async (): Promise<ApiResult<IndexingStats>> => {
    const result = await apiCall<Wire.IndexingStatsDto>('get_indexing_stats');
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: normalizeIndexingStats(result.data) };
  },

  /**
   * Lists all folders that have been indexed.
   * Returns folder paths with document counts and last indexed timestamps.
   *
   * @returns Array of IndexedFolder objects
   */
  getIndexedFolders: async (): Promise<ApiResult<IndexedFolder[]>> =>
    apiCall<Wire.IndexedFolder[]>('get_indexed_folders'),

  /**
   * Retrieves recent indexing activity history.
   * Shows files indexed, errors, and timestamps for debugging and monitoring.
   *
   * @param limit - Maximum number of activity records to return
   * @returns Array of IndexingActivity objects ordered by recency
   */
  getIndexingActivities: async (
    limit: number,
  ): Promise<ApiResult<IndexingActivity[]>> =>
    apiCall<Wire.IndexingActivity[]>('get_indexing_activities', { limit }),

  /**
   * Opens a file in the system's default application.
   * Uses platform-specific "open" command (e.g., xdg-open, open, start).
   *
   * @param path - Absolute path to file to open
   * @returns OpenFileResponseDto with success status and optional error
   */
  openFile: async (path: string): Promise<ApiResult<OpenFileResponseDto>> =>
    apiCall<OpenFileResponseDto>('open_file', { path }),

  /**
   * Opens a file by its internal document ID.
   * Resolves the ID to a file path and either opens in default application
   * or returns internal rendering info for web articles.
   *
   * @param fileId - Internal document identifier
   * @returns OpenFileResponseDto indicating action taken
   */
  openFileById: async (
    fileId: string,
  ): Promise<ApiResult<OpenFileResponseDto>> =>
    apiCall<OpenFileResponseDto>('open_file_by_id', { fileId }),

  /**
   * Retrieves the file system path for a document by its ID.
   * Useful for displaying paths or performing file operations.
   *
   * @param fileId - Internal document identifier
   * @returns Absolute file path as string
   */
  getFilePathById: async (fileId: string): Promise<ApiResult<string>> =>
    apiCall<string>('get_file_path_by_id', { fileId }),

  /**
   * Gets recently accessed documents ordered by last access time.
   * Useful for building "Recent Files" UI features.
   *
   * @param limit - Maximum number of recent documents to return
   * @returns Array of RecentDocument objects with paths and timestamps
   */
  getRecentDocuments: async (
    limit: number,
  ): Promise<ApiResult<RecentDocument[]>> =>
    apiCall<Wire.RecentDocument[]>('get_recent_documents', { limit }),

  /**
   * Lists ALL documents from the documents table.
   * This is the correct command for file browser UIs that need to show the complete library.
   *
   * Unlike getRecentDocuments which only returns tracked documents, this returns ALL documents
   * including newly imported web archives, PDFs, etc. that haven't been accessed yet.
   *
   * @param limit - Maximum number of documents to return (capped at 10000)
   * @returns Array of all documents with metadata
   */
  listAllDocuments: async (
    limit: number,
  ): Promise<ApiResult<DocumentMetadata[]>> =>
    apiCall<Wire.DocumentMetadataDto[]>('list_all_documents', { limit }),

  /**
   * Retrieves document metadata by ID.
   *
   * Used to get full document information (especially file path) when you only have
   * the document ID, such as from recent documents or search results.
   *
   * @param documentId - Unique document identifier (UUID)
   * @returns Document metadata including file path for opening
   *
   * @example
   * ```typescript
   * // Get document to open from recent list
   * const result = await VaultAPI.getDocument(docId);
   * if (result.ok && result.data.filePath) {
   *   const openResult = await VaultAPI.openFile(result.data.filePath);
   *   if (!openResult.ok) {
   *     console.error('Failed to open:', openResult.error);
   *   }
   * }
   * ```
   */
  getDocument: async (
    documentId: string,
  ): Promise<ApiResult<DocumentMetadata>> =>
    apiCall<DocumentMetadata>('get_document', { documentId }),

  /**
   * Reads the raw text content of a file from disk.
   * Returns UTF-8 decoded content for text-based formats.
   *
   * @param path - Absolute path to file to read
   * @returns File content as string
   */
  readFileContent: async (path: string): Promise<ApiResult<string>> =>
    apiCall<string>('read_file_content', { path }),

  /**
   * Reads the raw bytes of a file from disk.
   *
   * @param path - Absolute path to file to read
   * @returns File content as bytes
   */
  readFileBytes: async (path: string): Promise<ApiResult<Uint8Array>> => {
    const result = await apiCall<number[]>('read_file_bytes', { path });
    if (!result.ok) return result;
    return { ok: true, data: Uint8Array.from(result.data) };
  },

  /**
   * Retrieves file system metadata for a file.
   * Includes size, creation time, modification time, and permissions.
   *
   * @param path - Absolute path to file
   * @returns Metadata object with file system information
   */
  getFileMetadata: async (path: string): Promise<ApiResult<FileMetadata>> =>
    apiCall<Wire.FileMetadataDto>('get_file_metadata', { path }),

  /**
   * Opens the file's parent folder in the system file manager.
   * Highlights the file if the platform supports it (e.g., Finder, Explorer).
   *
   * @param path - Absolute path to file
   * @returns Void on success
   */
  showInFolder: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('show_in_folder', { path }),

  /**
   * Vault-wide type mix and recent growth.
   *
   * For surfaces that do not already hold the document list (Home, the
   * post-ingest sentence). The Library counts its own array so the counts and
   * the filter can never disagree.
   */
  getCorpusShape: async (): Promise<ApiResult<CorpusShapeDto>> =>
    apiCall<Wire.CorpusShapeDto>('get_corpus_shape'),

  /**
   * Conversations that have this document among their linked documents.
   *
   * @param documentId - The document to look up
   * @param limit - Maximum conversations to return (default 10, max 50)
   */
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
