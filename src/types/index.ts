import type { ApiError } from './api/result';


export type { SearchResultMetadata, SearchFilter, PerformanceStats, PerformanceReport } from './metadata';
export type { SearchResult } from './searchResult';

type SearchOptionsWire = import('../lib/bindings').SearchOptions;
export type SearchOptions = Pick<SearchOptionsWire, 'query'> & Partial<Omit<SearchOptionsWire, 'query'>>;

export type RecentDocument = import('../lib/bindings').RecentDocument;

// DocumentMetadata type matching list_all_documents backend response
// This matches DocumentMetadataDto from document_list.rs
export type DocumentMetadata = import('../lib/bindings').DocumentMetadataDto;

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

export type { IndexedFolder, IndexingActivity, IndexFileResponse } from './api/files';

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

export type {
  Conversation,
  ConversationMessage,
  ConversationalAnswer,
  CreateConversationParams,
  AskQuestionParams,
  StreamChunk,
  TurnMode,
  ToolPreferences,
  CompactionRecord,
} from './conversation';

export * from './api';

export type { ModelRecommendation, ModelSearchResult, CacheStats as ModelCatalogCacheStats } from './modelCatalog';

// Chat starters
export type { ChatStarter, ChatStarters } from './api/chatStarters';

// Transcription
export type { Transcript, TranscriptSegment, TranscriptionStatus } from './transcription';
