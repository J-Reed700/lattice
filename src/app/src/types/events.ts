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
    export const Started = z.object({
      id: z.string(),
      url: z.string().optional(),
      destination: z.string().optional(),
      total_bytes: z.number().nullable().optional(),
      model_name: z.string().optional(),
      model_id: z.string().optional(),
    });

    export const Progress = z.object({
      id: z.string(),
      bytes_downloaded: z.number(),
      total_bytes: z.number().nullable().optional(),
      bytes_per_second: z.number(),
      percentage: z.number().nullable().optional(),
      eta_seconds: z.number().nullable().optional(),
    });

    export const Paused = z.object({
      id: z.string(),
    });

    export const Resumed = z.object({
      id: z.string(),
    });

    export const Completed = z.object({
      id: z.string(),
      total_bytes_completed: z.number().optional(),
      elapsed_seconds: z.number().optional(),
    });

    export const Failed = z.object({
      id: z.string(),
      error: z.string(),
    });

    export const Cancelled = z.object({
      id: z.string(),
    });
  }

  export namespace Indexing {
    export const Progress = z.object({
      current: z.number(),
      total: z.number(),
      filename: z.string(),
    });

    export const Complete = z.object({
      path: z.string(),
      indexed_count: z.number(),
      duration_ms: z.number().optional(),
    });

    export const Error = z.object({
      message: z.string(),
      path: z.string(),
      filename: z.string().optional(),
    });

    export const Started = z.object({
      path: z.string(),
      total_files: z.number().optional(),
    });
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
  }

  export namespace LLM {
    export const StreamChunk = z.object({
      type: z.enum(['token', 'sources', 'done', 'error']),
      content: z.string().optional(),
      sources: z
        .array(
          z.object({
            file_path: z.string(),
            score: z.number(),
            snippet: z.string(),
          })
        )
        .optional(),
      message: z.string().optional(),
    });

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
     * Download started event
     * Emitted when a new download begins
     */
    export interface Started {
      id: string;
      url?: string;
      destination?: string;
      total_bytes?: number | null;
      model_name?: string;
      model_id?: string;
    }

    /**
     * Download progress event
     * Emitted periodically during active download
     */
    export interface Progress {
      id: string;
      bytes_downloaded: number;
      total_bytes?: number | null;
      bytes_per_second: number;
      percentage?: number | null;
      eta_seconds?: number | null;
    }

    /**
     * Download paused event
     * Emitted when user pauses a download
     */
    export interface Paused {
      id: string;
    }

    /**
     * Download resumed event
     * Emitted when user resumes a paused download
     */
    export interface Resumed {
      id: string;
    }

    /**
     * Download completed event
     * Emitted when download finishes successfully
     */
    export interface Completed {
      id: string;
      total_bytes_completed?: number;
      elapsed_seconds?: number;
    }

    /**
     * Download failed event
     * Emitted when download encounters an error
     */
    export interface Failed {
      id: string;
      error: string;
    }

    /**
     * Download cancelled event
     * Emitted when user cancels a download
     */
    export interface Cancelled {
      id: string;
    }
  }

  // ============================================================================
  // NAMESPACE: Indexing
  // ============================================================================

  export namespace Indexing {
    /**
     * Indexing progress event
     * Emitted during document indexing to show progress
     */
    export interface Progress {
      current: number;
      total: number;
      filename: string;
    }

    /**
     * Indexing completed event
     * Emitted when indexing finishes successfully
     */
    export interface Complete {
      path: string;
      indexed_count: number;
      duration_ms?: number;
    }

    /**
     * Indexing error event
     * Emitted when indexing encounters an error
     */
    export interface Error {
      message: string;
      path: string;
      filename?: string;
    }

    /**
     * Indexing started event
     * Emitted when indexing begins
     */
    export interface Started {
      path: string;
      total_files?: number;
    }
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
    Started: 'download:started' as const,
    Progress: 'download:progress' as const,
    Paused: 'download:paused' as const,
    Resumed: 'download:resumed' as const,
    Completed: 'download:completed' as const,
    Failed: 'download:failed' as const,
    Cancelled: 'download:cancelled' as const,
  },
  Indexing: {
    Progress: 'indexing-progress' as const,
    Complete: 'indexing-complete' as const,
    Error: 'indexing-error' as const,
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
