/**
 * Indexing status API types.
 *
 * `IndexingSnapshot` is the shape of both the `get_index_progress` command
 * (after `normalizeIndexProgress`) and the `indexing://progress` Tauri event,
 * so one type serves the poll and the push.
 */
import type { IndexStatus } from '../index';

/** One file that failed during the current indexing run. */
export interface IndexingFailure {
  /** Absolute path of the file that failed. */
  path: string;
  /** File name only, for display. */
  fileName: string;
  /** One-line reason, already trimmed by the backend. */
  reason: string;
  /** RFC3339 timestamp. */
  failedAt: string;
}

export interface IndexingSnapshot {
  totalFiles: number;
  processed: number;
  failed: number;
  currentFile?: string;
  status: IndexStatus;
  percentage: number;
  /** True while a run is paused. Drives Pause vs Resume. */
  paused: boolean;
  /** Newest first, capped at 50, cleared when a new run starts. */
  failures: IndexingFailure[];
}
