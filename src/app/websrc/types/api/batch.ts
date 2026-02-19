/**
 * Batch Job API Types
 *
 * Type definitions for batch import operations.
 * These types match the Rust backend structures from batch_history.rs
 */

export interface BatchJobSummary {
  jobId: string;
  // Legacy alias for compatibility with existing UI code paths
  id: string;
  jobType: string;
  status: string;
  totalItems: number;
  completedItems: number;
  failedItems: number;
  createdAt: string;
  completedAt?: string | null;
  progress: number;
}

export interface BatchJobStatus {
  jobId: string;
  // Legacy alias for compatibility with existing UI code paths
  id: string;
  jobType: string;
  status: string;
  totalItems: number;
  completedItems: number;
  failedItems: number;
  total_items: number;
  completed_items: number;
  failed_items: number;
  progress: number;
  createdAt: string;
  completedAt: string | null;
  items: BatchJobItem[];
}

export interface BatchJobItem {
  itemId: string;
  target: string;
  status: string;
  errorMessage: string | null;
  documentId: string | null;
  processedAt: string | null;
  // Legacy aliases
  id?: string;
  url?: string;
}

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
