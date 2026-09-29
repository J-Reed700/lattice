/**
 * Centralized Tauri Event Type System
 *
 * This file provides a type-safe, namespaced system for all Tauri IPC events.
 * All event listeners MUST use types from this file to ensure consistency.
 *
 * Architecture: Operation Bedrock
 * - Namespaced domains (Downloads, Models, Vault)
 * - Type-safe event names via TauriEventNames constant
 * - Runtime validation via Zod schemas
 * - Use `listen<TauriEvents.Namespace.EventType>()` from '@tauri-apps/api/event' for listening
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { z } from 'zod';


/**
 * Zod schemas for runtime validation of event payloads
 * Use these to validate events at the IPC boundary to catch
 * backend/frontend schema mismatches
 */
export namespace EventSchemas {
  export namespace Downloads {
    export const FileSnapshot = z.object({
      id: z.string(),
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

    export const Completed = z.object({
      modelId: z.string(),
      modelName: z.string(),
      totalSize: z.number(),
      fileCount: z.number(),
      timestamp: z.string(),
    });

    export const Failed = z.object({
      id: z.string(),
      error: z.string(),
    });

    export type FileSnapshot = z.infer<typeof FileSnapshot>;
    export type Single = z.infer<typeof Single>;
    export type Batch = z.infer<typeof Batch>;
    export type StateSnapshot = z.infer<typeof StateSnapshot>;
    export type Completed = z.infer<typeof Completed>;
    export type Failed = z.infer<typeof Failed>;
  }

  export namespace Models {
    /// Phases: `started → ready | skipped | failed`.
    export const WarmupStatus = z.object({
      role: z.enum(['chat', 'utility', 'embedding']),
      phase: z.enum(['started', 'ready', 'skipped', 'failed']),
      error: z.string().nullable().optional(),
    });
  }

  export namespace Vault {
    /// SQL save succeeded but markdown mirror failed.
    export const WriteError = z.object({
      noteId: z.string().nullable().optional(),
      target: z.string(),
      error: z.string(),
    });

    /// External `.md` edit imported into SQLite by the watcher.
    export const NoteImported = z.object({
      noteId: z.string(),
    });
  }
}


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
      id: string;
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
     * Download completed event
     * Emitted when a download is fully written and ready.
     * Note: uses camelCase to match Rust serialization.
     */
    export interface Completed {
      modelId: string;
      modelName: string;
      totalSize: number;
      fileCount: number;
      timestamp: string;
    }

    /**
     * Download failed event
     * Emitted when a download fails
     */
    export interface Failed {
      id: string;
      error: string;
    }
  }
}


/**
 * Type-safe event name constants
 * Use these instead of string literals for event names
 */
export const TauriEventNames = {
  Downloads: {
    // Progress snapshots for every download, discriminated by payload.kind and payload.status
    Event: 'download:progress' as const,
    // Terminal events, emitted alongside the final progress snapshot
    Completed: 'download:completed' as const,
    Failed: 'download:failed' as const,
  },
  Indexing: {
    // Single event name for all indexing events (discriminated by payload.type)
    Event: 'indexing-progress' as const,
  },
  Models: {
    /// Boot-time pre-warm status for chat/utility/embedding model roles.
    /// Backend fires `started → ready|skipped|failed` per role.
    WarmupStatus: 'model:warmup-status' as const,
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
  Vault: {
    WriteError: 'vault:write-error' as const,
    NoteImported: 'vault:note-imported' as const,
  },
} as const;


/**
 * `status` values on `llm-stream` events that carry something other than text.
 * `Verification` arrives after the turn has returned: the answer's grounding
 * check runs in the background and reports on the turn's own request id.
 */
export const ChatStreamStatus = {
  Step: 'step' as const,
  Retrieval: 'retrieval' as const,
  Verification: 'verification' as const,
} as const;


/**
 * Listen to a Tauri event with runtime validation of the payload
 *
 * This catches backend/frontend schema mismatches at runtime by validating
 * the event payload against a Zod schema before calling the handler.
 *
 * @example
 * ```typescript
 * const unlisten = await listenValidated(
 *   TauriEventNames.Downloads.Event,
 *   EventSchemas.Downloads.StateSnapshot,
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
