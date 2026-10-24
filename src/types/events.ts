import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { z } from 'zod';

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
    // Phases: started → ready | skipped | failed
    export const WarmupStatus = z.object({
      role: z.enum(['chat', 'utility', 'embedding', 'learning']),
      phase: z.enum(['started', 'ready', 'skipped', 'failed']),
      error: z.string().nullable().optional(),
    });
  }

  export namespace Vault {
    // SQL save succeeded but markdown mirror failed.
    export const WriteError = z.object({
      noteId: z.string().nullable().optional(),
      target: z.string(),
      error: z.string(),
    });

    // External .md edit imported into SQLite by the watcher.
    export const NoteImported = z.object({
      noteId: z.string(),
    });
  }

  export namespace WebPageSnapshot {
    // One frozen copy of a cited page, as WebPageSnapshotDto in the bindings.
    export const Snapshot = z.object({
      id: z.string(),
      conversationId: z.string(),
      requestedUrl: z.string(),
      finalUrl: z.string().nullable(),
      normalizedUrl: z.string(),
      title: z.string().nullable(),
      status: z.enum(['pending', 'ready', 'failed']),
      method: z.string().nullable(),
      capturedAt: z.string().nullable(),
      byteSize: z.number().nullable(),
      sha256: z.string().nullable(),
      resourceCount: z.number(),
      missingResourceCount: z.number(),
      partial: z.boolean(),
      partialReasons: z.array(z.string()),
      errorCode: z.string().nullable(),
      errorMessage: z.string().nullable(),
      faviconDataUrl: z.string().nullable(),
      progressMessage: z.string().nullable(),
      verified: z.boolean().nullable(),
      readerText: z.string().nullable().optional(),
    });

    // Every status flip and progress step of a capture, debounced by the backend.
    export const Changed = z.object({
      conversationId: z.string(),
      url: z.string(),
      snapshot: Snapshot,
    });

    // A link clicked inside a frozen page. The page itself never navigates.
    export const Link = z.object({
      snapshotId: z.string(),
      url: z.string(),
    });
  }
}


export namespace TauriEvents {
  export namespace Downloads {
    export type DownloadStatus = 'pending' | 'downloading' | 'paused' | 'completed' | 'error' | 'cancelled';

    export type FileStatus = 'pending' | 'downloading' | 'completed' | 'error';

    export interface FileSnapshot {
      id: string;
      filename: string;
      bytesDownloaded: number;
      totalBytes: number;
      status: FileStatus;
    }

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

    export type StateSnapshot = Single | Batch;

    export interface Completed {
      modelId: string;
      modelName: string;
      totalSize: number;
      fileCount: number;
      timestamp: string;
    }

    export interface Failed {
      id: string;
      error: string;
    }
  }

  export namespace WebPageSnapshot {
    export type Snapshot = z.infer<typeof EventSchemas.WebPageSnapshot.Snapshot>;
    export type Changed = z.infer<typeof EventSchemas.WebPageSnapshot.Changed>;
    export type Link = z.infer<typeof EventSchemas.WebPageSnapshot.Link>;
  }
}

export const TauriEventNames = {
  Downloads: {
    Event: 'download:progress' as const,
    Completed: 'download:completed' as const,
    Failed: 'download:failed' as const,
  },
  Indexing: {
    Event: 'indexing-progress' as const,
  },
  Models: {
    // Boot-time pre-warm status for chat/utility/embedding model roles (started → ready|skipped|failed).
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
  WebPageSnapshot: {
    Changed: 'web-page-snapshot:changed' as const,
    Link: 'web-page-snapshot:link' as const,
  },
} as const;

/**
 * `status` values on `llm-stream` events that carry something other than text.
 * `Captured` and `Verification` arrive after the turn has returned, in that
 * order: the cited pages the answer never read are captured, then its
 * grounding check runs, both in the background and reporting on the turn's own
 * request id.
 */
export const ChatStreamStatus = {
  Step: 'step' as const,
  Retrieval: 'retrieval' as const,
  Captured: 'captured' as const,
  Verification: 'verification' as const,
} as const;

/**
 * Listen to a Tauri event with runtime validation of the payload.
 * Catches backend/frontend schema mismatches by validating against a Zod schema.
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
