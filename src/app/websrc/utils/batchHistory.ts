import { VaultAPI } from '@/lib/api';
import type { BatchJobSummary, BatchJobStatus } from '@/types/api/batch';

export async function listBatchJobs(
  limit?: number,
  offset?: number
): Promise<BatchJobSummary[]> {
  const result = await VaultAPI.listBatchJobs(limit, offset);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

export async function deleteBatchJob(jobId: string): Promise<void> {
  const result = await VaultAPI.deleteBatchJob(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
}

export async function retryFailedItems(jobId: string): Promise<string> {
  const result = await VaultAPI.retryFailedBatchItems(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

export async function getBatchJobDetails(jobId: string): Promise<BatchJobStatus> {
  const result = await VaultAPI.getBatchJobStatus(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}
