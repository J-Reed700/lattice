/**
 * Centralized Tauri Event Type System
 *
 * This file provides a type-safe, namespaced system for all Tauri IPC events.
 * All event listeners MUST use types from this file to ensure consistency.
 *
 * Architecture: Operation Bedrock
 * - 7 namespaced domains (Downloads, Indexing, Models, Progress, LLM, FileWatch, Search)
 * - Type-safe event names via TauriEventNames constant
 * - Runtime validation via Zod schemas
 * - Use `listen<TauriEvents.Namespace.EventType>()` from '@tauri-apps/api/event' for listening
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { z } from 'zod';

// ============================================================================
// ZOD RUNTIME VALIDATION SCHEMAS
// ============================================================================

/**
 * Zod schemas for runtime validation of event payloads
 * Use these to validate events at the IPC boundary to catch
 * backend/frontend schema mismatches
 */
export namespace EventSchemas {
  export namespace Downloads {
    export const FileSnapshot = z.object({
      filename: z.string(),
      bytesDownloaded: z.number(),
      totalBytes: z.number(),
      status: z.enum(['pending', 'downloading', 'completed', 'error']),
    });

    export const Single = z.object({
      kind: z.literal('single'),
      id: z.string(),
      filename: z.string(),
      bytesDownloaded: z.number(),
      totalBytes: z.number().nullable(),
      bytesPerSecond: z.number(),
      percentage: z.number().nullable(),
      etaSeconds: z.number().nullable(),
      status: z.enum(['pending', 'downloading', 'paused', 'completed', 'error', 'cancelled']),
    });

    export const Batch = z.object({
      kind: z.literal('batch'),
      id: z.string(),
      groupName: z.string(),
      files: z.array(FileSnapshot),
      totalFiles: z.number(),
      completedFiles: z.number(),
      aggregateBytesDownloaded: z.number(),
      aggregateTotalBytes: z.number(),
      aggregateBytesPerSecond: z.number(),
      aggregatePercentage: z.number(),
      aggregateEtaSeconds: z.number().nullable(),
      status: z.enum(['pending', 'downloading', 'paused', 'completed', 'error', 'cancelled']),
    });

    export const StateSnapshot = z.discriminatedUnion('kind', [Single, Batch]);

    export const Failed = z.object({
      id: z.string(),
      error: z.string(),
    });

    export type FileSnapshot = z.infer<typeof FileSnapshot>;
    export type Single = z.infer<typeof Single>;
    export type Batch = z.infer<typeof Batch>;
    export type StateSnapshot = z.infer<typeof StateSnapshot>;
    export type Failed = z.infer<typeof Failed>;
  }

  export namespace Indexing {
    // Indexing events use a discriminated union with 'type' field
    // Matches Rust IndexingEvent enum in src/crates/recall/infrastructure/indexing/modules/events.rs

    export const Started = z.object({
      type: z.literal('Started'),
      total_files: z.number(),
    });

    export const FileStarted = z.object({
      type: z.literal('FileStarted'),
      path: z.string(),
      current: z.number(),
      total: z.number(),
    });

    export const FileCompleted = z.object({
      type: z.literal('FileCompleted'),
      path: z.string(),
      chunks: z.number(),
      duration_ms: z.number(),
      current: z.number(),
      total: z.number(),
    });

    export const FileError = z.object({
      type: z.literal('FileError'),
      path: z.string(),
      error: z.string(),
      current: z.number(),
      total: z.number(),
    });

    export const Completed = z.object({
      type: z.literal('Completed'),
      total_files: z.number(),
      total_chunks: z.number(),
      duration_ms: z.number(),
    });

    export const Cancelled = z.object({
      type: z.literal('Cancelled'),
    });

    // Union of all indexing event variants
    export const Event = z.discriminatedUnion('type', [
      Started,
      FileStarted,
      FileCompleted,
      FileError,
      Completed,
      Cancelled,
    ]);

    // Legacy alias for backward compatibility
    export const Progress = FileStarted;
    export const Complete = Completed;
    export const Error = FileError;
  }

  export namespace Models {
    export const DownloadProgress = z.object({
      percentage: z.number(),
      speedMbps: z.number().optional(),
      etaSeconds: z.number().optional(),
    });

    export const DownloadCompleted = z.object({
      modelId: z.string(),
      modelName: z.string(),
      totalSize: z.number(),
      fileCount: z.number(),
      timestamp: z.string(),
    });

    export const DownloadError = z.object({
      model_id: z.string(),
      error: z.string(),
    });
  }

  export namespace Progress {
    export const Generic = z.object({
      current: z.number(),
      total: z.number(),
      message: z.string().optional(),
      data: z.unknown().optional(),
    });

    // Generic progress event for operations (upload, indexing, search, export, OCR)
    export const ProgressEvent = z.object({
      operationId: z.string().optional(),
      current: z.number(),
      total: z.number(),
      filename: z.string().optional(),
      message: z.string().optional(),
      percentage: z.number().optional(),
      eta_ms: z.number().optional(),
    });

    // Generic completion event for operations
    export const CompleteEvent = z.object({
      operationId: z.string().optional(),
      path: z.string().optional(),
      message: z.string().optional(),
    });

    // Generic error event for operations
    export const ErrorEvent = z.object({
      operationId: z.string().optional(),
      message: z.string(),
      path: z.string().optional(),
    });
  }

  export namespace LLM {
    export const StreamChunk = z.discriminatedUnion('type', [
      z.object({
        type: z.literal('token'),
        content: z.string(),
      }),
      z.object({
        type: z.literal('sources'),
        sources: z.array(z.object({
          file_path: z.string(),
          score: z.number(),
          snippet: z.string(),
        })),
      }),
      z.object({
        type: z.literal('done'),
      }),
      z.object({
        type: z.literal('error'),
        message: z.string(),
      }),
    ]);

    export const QueryStarted = z.object({
      query: z.string(),
      model: z.string(),
      timestamp: z.number(),
    });

    export const QueryCompleted = z.object({
      query: z.string(),
      response: z.string(),
      model: z.string(),
      tokens_used: z.number(),
      duration_ms: z.number(),
    });
  }

  export namespace FileWatch {
    export const Event = z.object({
      action: z.enum(['added', 'modified', 'removed']),
      path: z.string(),
      timestamp: z.number(),
    });

    export const Started = z.object({
      path: z.string(),
      recursive: z.boolean(),
    });

    export const Error = z.object({
      path: z.string(),
      error: z.string(),
    });
  }

  export namespace Search {
    export const Started = z.object({
      query: z.string(),
      mode: z.enum(['semantic', 'keyword', 'hybrid']),
      timestamp: z.number(),
    });

    export const Complete = z.object({
      query: z.string(),
      result_count: z.number(),
      time_ms: z.number(),
    });
  }
}

// ============================================================================
// NAMESPACE: Downloads
// ============================================================================

export namespace TauriEvents {
  export namespace Downloads {
    /**
     * Download status for both single and batch downloads
     */
    export type DownloadStatus = 'pending' | 'downloading' | 'paused' | 'completed' | 'error' | 'cancelled';

    /**
     * Per-file status for multi-file downloads
     */
    export type FileStatus = 'pending' | 'downloading' | 'completed' | 'error';

    /**
     * Per-file progress information for batch downloads
     */
    export interface FileSnapshot {
      filename: string;
      bytesDownloaded: number;
      totalBytes: number;
      status: FileStatus;
    }

    /**
     * Single file download snapshot
     *
     * Represents simple downloads with one file (PDFs, images, documents).
     * Architecturally distinct from Batch which handles multi-file model downloads.
     */
    export interface Single {
      kind: 'single';
      id: string;
      filename: string;
      bytesDownloaded: number;
      totalBytes: number | null;
      bytesPerSecond: number;
      percentage: number | null;
      etaSeconds: number | null;
      status: DownloadStatus;
    }

    /**
     * Multi-file download batch snapshot
     *
     * Represents complex downloads with multiple files (LLM models with tokenizer, config, weights).
     * Provides aggregate metrics across all files in the batch.
     */
    export interface Batch {
      kind: 'batch';
      id: string;
      groupName: string;
      files: FileSnapshot[];
      totalFiles: number;
      completedFiles: number;
      aggregateBytesDownloaded: number;
      aggregateTotalBytes: number;
      aggregateBytesPerSecond: number;
      aggregatePercentage: number;
      aggregateEtaSeconds: number | null;
      status: DownloadStatus;
    }

    /**
     * Discriminated union of download snapshots
     */
    export type StateSnapshot = Single | Batch;

    /**
     * Download failed event
     * Emitted when a download fails
     */
    export interface Failed {
      id: string;
      error: string;
    }
  }

  // ============================================================================
  // NAMESPACE: Indexing
  // ============================================================================

  export namespace Indexing {
    /**
     * Indexing started event
     * Emitted when indexing begins
     */
    export interface Started {
      type: 'Started';
      total_files: number;
    }

    /**
     * File started event
     * Emitted when a file begins indexing
     */
    export interface FileStarted {
      type: 'FileStarted';
      path: string;
      current: number;
      total: number;
    }

    /**
     * File completed event
     * Emitted when a file finishes indexing
     */
    export interface FileCompleted {
      type: 'FileCompleted';
      path: string;
      chunks: number;
      duration_ms: number;
      current: number;
      total: number;
    }

    /**
     * File error event
     * Emitted when a file fails to index
     */
    export interface FileError {
      type: 'FileError';
      path: string;
      error: string;
      current: number;
      total: number;
    }

    /**
     * Indexing completed event
     * Emitted when all indexing finishes successfully
     */
    export interface Completed {
      type: 'Completed';
      total_files: number;
      total_chunks: number;
      duration_ms: number;
    }

    /**
     * Indexing cancelled event
     * Emitted when indexing is cancelled by user
     */
    export interface Cancelled {
      type: 'Cancelled';
    }

    /**
     * Union type of all indexing events (discriminated by 'type' field)
     */
    export type Event =
      | Started
      | FileStarted
      | FileCompleted
      | FileError
      | Completed
      | Cancelled;

    // Legacy aliases for backward compatibility
    /** @deprecated Use FileStarted instead */
    export type Progress = FileStarted;
    /** @deprecated Use Completed instead */
    export type Complete = Completed;
    /** @deprecated Use FileError instead */
    export type Error = FileError;
  }

  // ============================================================================
  // NAMESPACE: Models
  // ⚠️ DEPRECATED: Model download events have moved to TauriEvents.Downloads.*
  // ============================================================================

  /**
   * NAMESPACE: Models
   *
   * ⚠️ DEPRECATED: Model download events have moved to TauriEvents.Downloads.*
   * These types are kept for backward compatibility only.
   *
   * Migration:
   * - TauriEvents.Models.DownloadProgress → TauriEvents.Downloads.Progress
   * - TauriEvents.Models.DownloadCompleted → TauriEvents.Downloads.Completed
   * - TauriEvents.Models.DownloadError → TauriEvents.Downloads.Failed
   */
  export namespace Models {
    /**
     * @deprecated Use TauriEvents.Downloads.Progress instead
     *
     * Model download progress event
     * For backward compatibility with ModelDownloadScreen
     */
    export interface DownloadProgress {
      percentage: number;
      speedMbps?: number;
      etaSeconds?: number;
    }

    /**
     * @deprecated Use TauriEvents.Downloads.Completed instead
     *
     * Model download completed event
     * Emitted when a model is fully downloaded and ready
     * Note: Uses camelCase to match Rust serialization
     */
    export interface DownloadCompleted {
      modelId: string;
      modelName: string;
      totalSize: number;
      fileCount: number;
      timestamp: string;
    }

    /**
     * @deprecated Use TauriEvents.Downloads.Failed instead
     *
     * Model download error event
     * Emitted when model download fails
     */
    export interface DownloadError {
      model_id: string;
      error: string;
    }
  }

  // ============================================================================
  // NAMESPACE: Progress (Generic)
  // ============================================================================

  export namespace Progress {
    /**
     * Generic progress event
     * Used for long-running operations (e.g., batch jobs)
     */
    export interface Generic<TData = unknown> {
      current: number;
      total: number;
      message?: string;
      data?: TData;
    }
  }

  // ============================================================================
  // NAMESPACE: LLM
  // ============================================================================

  export namespace LLM {
    /**
     * LLM stream chunk event
     * Emitted during streaming LLM responses
     */
    export interface StreamChunk {
      type: 'token' | 'sources' | 'done' | 'error';
      content?: string;
      sources?: Array<{
        file_path: string;
        score: number;
        snippet: string;
      }>;
      message?: string;
    }

    /**
     * LLM query started event
     * Emitted when LLM query begins
     */
    export interface QueryStarted {
      query: string;
      model: string;
      timestamp: number;
    }

    /**
     * LLM query completed event
     * Emitted when LLM query finishes
     */
    export interface QueryCompleted {
      query: string;
      response: string;
      model: string;
      tokens_used: number;
      duration_ms: number;
    }
  }

  // ============================================================================
  // NAMESPACE: FileWatch
  // ============================================================================

  export namespace FileWatch {
    /**
     * File watch event
     * Emitted when watched file system changes occur
     */
    export interface Event {
      action: 'added' | 'modified' | 'removed';
      path: string;
      timestamp: number;
    }

    /**
     * File watch started event
     * Emitted when file watching begins for a directory
     */
    export interface Started {
      path: string;
      recursive: boolean;
    }

    /**
     * File watch error event
     * Emitted when file watching encounters an error
     */
    export interface Error {
      path: string;
      error: string;
    }
  }

  // ============================================================================
  // NAMESPACE: Search
  // ============================================================================

  export namespace Search {
    /**
     * Search started event
     * Emitted when a search query begins
     */
    export interface Started {
      query: string;
      mode: 'semantic' | 'keyword' | 'hybrid';
      timestamp: number;
    }

    /**
     * Search completed event
     * Emitted when search finishes
     */
    export interface Complete {
      query: string;
      result_count: number;
      time_ms: number;
    }
  }
}

// ============================================================================
// EVENT NAME CONSTANTS
// ============================================================================

/**
 * Type-safe event name constants
 * Use these instead of string literals for event names
 */
export const TauriEventNames = {
  Downloads: {
    // Single event name for all download events (discriminated by payload.kind and payload.status)
    Event: 'download:progress' as const,

    // Legacy event names for backward compatibility
    /** @deprecated Use Event instead - discriminate by payload.status */
    Started: 'download:started' as const,
    /** @deprecated Use Event instead - status is in payload */
    Progress: 'download:progress' as const,
    /** @deprecated Use Event instead - status is in payload */
    Paused: 'download:paused' as const,
    /** @deprecated Use Event instead - status is in payload */
    Resumed: 'download:resumed' as const,
    /** @deprecated Use Event instead - status is in payload */
    Completed: 'download:completed' as const,
    /** @deprecated Use Event instead - status is in payload */
    Failed: 'download:failed' as const,
    /** @deprecated Use Event instead - status is in payload */
    Cancelled: 'download:cancelled' as const,
  },
  Indexing: {
    // Single event name for all indexing events (discriminated by payload.type)
    Event: 'indexing-progress' as const,

    // Legacy aliases for backward compatibility
    /** @deprecated Use Event instead */
    Progress: 'indexing-progress' as const,
    /** @deprecated Event type is now discriminated by payload.type field */
    Complete: 'indexing-complete' as const,
    /** @deprecated Event type is now discriminated by payload.type field */
    Error: 'indexing-error' as const,
    /** @deprecated Event type is now discriminated by payload.type field */
    Started: 'indexing-started' as const,
  },
  Models: {
    // ⚠️ DEPRECATED: Use TauriEventNames.Downloads.* instead
    // Legacy event names kept for reference during migration
    DownloadProgress: 'download:progress' as const,
    DownloadCompleted: 'download:completed' as const,
    DownloadError: 'download:failed' as const,
  },
  LLM: {
    StreamChunk: 'llm-stream' as const,
    QueryStarted: 'llm-query-started' as const,
    QueryCompleted: 'llm-query-completed' as const,
  },
  FileWatch: {
    Event: 'file-watch-event' as const,
    Started: 'file-watch-started' as const,
    Error: 'file-watch-error' as const,
  },
  Search: {
    Started: 'search-started' as const,
    Complete: 'search-complete' as const,
  },
} as const;

// ============================================================================
// RUNTIME VALIDATION UTILITIES
// ============================================================================

/**
 * Listen to a Tauri event with runtime validation of the payload
 *
 * This catches backend/frontend schema mismatches at runtime by validating
 * the event payload against a Zod schema before calling the handler.
 *
 * @example
 * ```typescript
 * const unlisten = await listenValidated(
 *   TauriEventNames.Downloads.Progress,
 *   EventSchemas.Downloads.Progress,
 *   (event) => {
 *     // event.payload is guaranteed to match the schema
 *     console.log('Progress:', event.payload.bytes_downloaded);
 *   },
 *   (error) => {
 *     // Handle validation errors
 *     console.error('Invalid event payload:', error);
 *   }
 * );
 * ```
 */
export async function listenValidated<T>(
  eventName: string,
  schema: z.ZodType<T>,
  handler: (event: { payload: T }) => void,
  onValidationError?: (error: z.ZodError) => void
): Promise<UnlistenFn> {
  return listen<unknown>(eventName, (event) => {
    const result = schema.safeParse(event.payload);

    if (result.success) {
      handler({ payload: result.data });
    } else {
      if (onValidationError) {
        onValidationError(result.error);
      } else {
        console.error(
          `[IPC Event Validation Error] Event '${eventName}' failed validation:`,
          result.error.format()
        );
      }
    }
  });
}

// ============================================================================
// BACKWARD COMPATIBILITY EXPORTS
// ============================================================================

/**
 * @deprecated Use TauriEvents.Indexing.Progress instead
 */
export type IndexingProgressEvent = TauriEvents.Indexing.Progress;

/**
 * @deprecated Use TauriEvents.Indexing.Complete instead
 */
export type IndexingCompleteEvent = TauriEvents.Indexing.Complete;

/**
 * @deprecated Use TauriEvents.Indexing.Error instead
 */
export type IndexingErrorEvent = TauriEvents.Indexing.Error;

/**
 * @deprecated Use TauriEvents.Models.DownloadProgress instead
 */
export type ModelDownloadProgressEvent = TauriEvents.Models.DownloadProgress;

/**
 * @deprecated Use TauriEvents.FileWatch.Event instead
 */
export type FileWatchEvent = TauriEvents.FileWatch.Event;

/**
 * @deprecated Use TauriEvents.Search.Started instead
 */
export type SearchStartedEvent = TauriEvents.Search.Started;

/**
 * @deprecated Use TauriEvents.Search.Complete instead
 */
export type SearchCompleteEvent = TauriEvents.Search.Complete;
