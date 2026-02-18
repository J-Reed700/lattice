import type { ApiError } from './api/result';
import type { SearchResultMetadata, SearchFilter } from './metadata';


export type { SearchResultMetadata, SearchFilter, PerformanceStats, PerformanceReport } from './metadata';

export interface SearchOptions {
  query: string;
  limit?: number;
  filter?: SearchFilter;
  searchMode?: 'semantic' | 'keyword' | 'hybrid';
}

export interface SearchResult {
  id: string;
  title: string;           // Direct field from Rust SearchResultDto
  content: string;
  highlights?: string[];
  score: number;
  path: string | null | undefined;           // Rust Option<String> serializes to null
  documentId: string | null | undefined;
  position: number | null | undefined;       // Rust Option<usize> serializes to null
  vectorScore: number | null | undefined;
  bm25Score: number | null | undefined;
  vectorRank: number | null | undefined;
  bm25Rank: number | null | undefined;
  metadata: SearchResultMetadata | any;      // Allow both typed and untyped metadata
}

// Add RecentDocument type to match backend
export interface RecentDocument {
  id: string;
  documentId: string;
  documentName: string;
  documentPath: string;
  fileType: string | null;
  lastAccessedAt: string;
  accessCount: number;
  // Additional fields used by Dashboard components
  fileName: string;
  filePath: string;
  indexedAt: string;
  modifiedAt: string;
  sizeBytes: number;
}

// DocumentMetadata type matching list_all_documents backend response
// This matches DocumentMetadataDto from document_list.rs
export interface DocumentMetadata {
  id: string;
  fileName: string;
  filePath: string;
  fileType: string;
  category: string;
  language: string;
  modifiedAt: string;
  indexedAt: string;
  wordCount: number;
}

export interface IndexProgress {
  totalFiles: number;
  processed: number;
  failed: number;
  currentFile?: string;
  status: IndexStatus;
  percentage: number;
  estimatedRemainingMs?: number;
}

export type IndexStatus = 'idle' | 'scanning' | 'processing' | 'complete' | 'error' | 'cancelled';

export interface IndexingStats {
  indexedDocuments: number;
  totalChunks: number;
}

// Import types from API definitions (use snake_case to match Rust backend)
export type { IndexedFolder, IndexingActivity, IndexFileResponse } from './api/files';

export interface AppConfig {
  indexedPaths: string[];
  excludePatterns: string[];
  autoIndex: boolean;
  ollamaEndpoint: string;
  ollamaModel: string;
}

export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string; details?: ApiError };

export interface WebIngestResponse {
  documentId: string;
  url: string;
  title: string;
  wordCount: number;
  chunks: number;
  siteName: string | null;
  author: string | null;
  readingTimeMinutes: number | null;
}

// Export conversation types
export type {
  Conversation,
  ConversationMessage,
  ConversationalAnswer,
  CreateConversationParams,
  AskQuestionParams,
  StreamChunk,
  TurnMode,
  ToolPreferences,
} from './conversation';

// Export all API types
export * from './api';

// Export model catalog types
export type { ModelRecommendation, ModelSearchResult, CacheStats as ModelCatalogCacheStats } from './modelCatalog';
