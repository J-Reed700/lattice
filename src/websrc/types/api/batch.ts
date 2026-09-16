/**
 * Batch Job API Types
 *
 * Type definitions for batch import operations.
 * These types match the Rust backend structures from batch_history.rs
 */

export type BatchJobSummary = import('../../lib/bindings').BatchJobSummaryDto & {
  id: string;
  progress: number;
};

export type BatchJobStatus = Omit<import('../../lib/bindings').BatchJobStatusDto, 'items'> & {
  id: string;
  total_items: number;
  completed_items: number;
  failed_items: number;
  progress: number;
  items: BatchJobItem[];
};

export type BatchJobItem = import('../../lib/bindings').BatchJobItemDto & { id?: string; url?: string };

export interface ListBatchJobsResponse {
  jobs: BatchJobSummary[];
}

export interface DeleteBatchJobResponse {
  success: boolean;
}

export interface RetryFailedItemsResponse {
  newJobId: string;
  retriedCount: number;
}

export interface CancelBatchJobResponse {
  cancelledCount: number;
}
