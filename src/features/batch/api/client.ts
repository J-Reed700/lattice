import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, BatchJobSummary, BatchJobStatus } from '@/types';

function computeBatchProgress(
  totalItems: number,
  completedItems: number,
  failedItems: number,
): number {
  if (totalItems <= 0) {
    return 0;
  }
  const processed = completedItems + failedItems;
  return Math.min(100, Math.max(0, (processed / totalItems) * 100));
}

function normalizeBatchStatus(raw: Wire.BatchJobStatusDto): BatchJobStatus {
  return {
    ...raw,
    id: raw.jobId,
    total_items: raw.totalItems,
    completed_items: raw.completedItems,
    failed_items: raw.failedItems,
    progress: computeBatchProgress(
      raw.totalItems,
      raw.completedItems,
      raw.failedItems,
    ),
    completedAt: raw.completedAt ?? null,
    items: raw.items.map((item) => ({
      ...item,
      id: item.itemId,
      url: item.target,
    })),
  };
}

function normalizeBatchSummary(raw: Wire.BatchJobSummaryDto): BatchJobSummary {
  return {
    ...raw,
    id: raw.jobId,
    progress: computeBatchProgress(
      raw.totalItems,
      raw.completedItems,
      raw.failedItems,
    ),
  };
}
export const batchApi = {
  /**
   * Lists all batch jobs with pagination support.
   * Returns batch job history ordered by creation date (newest first).
   *
   * @param limit - Max jobs to return (default: 20)
   * @param offset - Skip N jobs for pagination (default: 0)
   * @returns Array of batch job summaries
   */
  listBatchJobs: async (
    limit?: number,
    offset?: number,
  ): Promise<ApiResult<BatchJobSummary[]>> => {
    const result = await apiCall<Wire.ListBatchJobsResponseDto>(
      'get_batch_history',
      { request: { limit, offset } },
    );
    if (!result.ok) {
      return result;
    }
    return {
      ok: true,
      data: (result.data.jobs || []).map(normalizeBatchSummary),
    };
  },

  /**
   * Deletes a batch job and all its associated items.
   * Permanent operation - cannot be undone.
   *
   * @param jobId - ID of batch job to delete
   * @returns Void on success
   */
  deleteBatchJob: async (jobId: string): Promise<ApiResult<void>> => {
    const result = await apiCall<Wire.DeleteBatchJobResponseDto>(
      'delete_batch_job',
      { jobId },
    );
    if (!result.ok) {
      return result;
    }
    if (!result.data.success) {
      return { ok: false, error: 'Failed to delete batch job' };
    }
    return { ok: true, data: undefined };
  },

  /**
   * Retries failed items through the matching importer. File retries update
   * their original job; an optional path replaces one selected failed file.
   *
   * @param jobId - ID of job with failed items
   * @returns New job ID for the retry operation
   */
  retryFailedBatchItems: async (
    jobId: string,
    itemId?: string,
    replacementPath?: string,
  ): Promise<ApiResult<string>> => {
    const result = await apiCall<Wire.RetryFailedItemsResponseDto>(
      'retry_failed_items',
      { jobId, itemId, replacementPath },
    );
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.newJobId };
  },

  /**
   * Gets detailed status of a batch job including all items.
   * Returns comprehensive information about job progress and item statuses.
   *
   * @param jobId - ID of batch job to query
   * @returns Detailed batch job status with all items
   */
  getBatchJobStatus: async (
    jobId: string,
  ): Promise<ApiResult<BatchJobStatus>> => {
    const result = await apiCall<Wire.BatchJobStatusDto>('get_batch_status', {
      request: { jobId },
    });
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: normalizeBatchStatus(result.data) };
  },

  /**
   * Cancels an ongoing batch job.
   * Stops processing and marks job as cancelled.
   *
   * @param jobId - ID of batch job to cancel
   * @returns Number of items cancelled
   */
  cancelBatchJob: async (jobId: string): Promise<ApiResult<number>> => {
    const result = await apiCall<Wire.CancelBatchJobResponseDto>(
      'cancel_batch',
      { request: { jobId } },
    );
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.cancelledCount };
  },

  /**
   * Starts a batch import of files from disk.
   * Indexes multiple files in a single batch operation with progress tracking.
   *
   * @param filePaths - Array of absolute file paths to import
   * @param ownerConversationId - Set when these files were attached to a chat
   *   rather than added to the library. Files this job imports then belong to
   *   that conversation: they stay out of the library and out of every other
   *   chat's searches, and they are deleted with it. Leave it out for a real
   *   library import.
   * @returns Batch job ID for tracking progress
   */
  startBatchFileImport: async (
    filePaths: string[],
    ownerConversationId?: string,
  ): Promise<ApiResult<string>> => {
    const result = await apiCall<Wire.StartBatchFileImportResponseDto>(
      'batch_import_files',
      {
        request: { filePaths, ownerConversationId },
      },
    );
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.jobId };
  },

  /**
   * Starts a batch import of URLs.
   * Fetches and indexes multiple URLs in a single batch operation with progress tracking.
   *
   * @param urls - Array of URLs to import
   * @returns Batch job ID for tracking progress
   */
  startBatchUrlImport: async (
    urls: string[],
    options?: { extractArticle?: boolean },
  ): Promise<ApiResult<string>> => {
    const result = await apiCall<Wire.StartBatchUrlImportResponseDto>(
      'batch_import_urls',
      {
        request: {
          urls,
          options: options ? { extractArticle: options.extractArticle } : null,
        },
      },
    );
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.jobId };
  },

  /**
   * Starts a batch file import operation for indexing multiple files
   * @param filePaths - Array of absolute file paths to index
   * @returns Operation ID for tracking progress
   */
  batchFileImport: async (
    filePaths: string[],
    spaceId?: string,
    indexing?: Wire.FileIndexingOptionsDto,
  ): Promise<ApiResult<string>> => {
    const result = await apiCall<Wire.StartBatchFileImportResponseDto>(
      'batch_import_files',
      {
        request: { filePaths, spaceId, indexing },
      },
    );
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.jobId };
  },
};
