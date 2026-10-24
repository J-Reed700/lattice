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
