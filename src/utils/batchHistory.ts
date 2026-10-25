import { VaultAPI, type ApiClient } from '@/lib/api';
import type { BatchJobSummary, BatchJobStatus } from '@/types/api/batch';

/** Include older unresolved failures, even after more than 100 later imports. */
export async function listAllBatchJobs(api: ApiClient = VaultAPI): Promise<BatchJobSummary[]> {
  const jobs: BatchJobSummary[] = [];
  for (let offset = 0; ; offset += 100) {
    const page = await listBatchJobs(100, offset, api);
    jobs.push(...page);
    if (page.length < 100) return jobs;
  }
}

export async function listBatchJobs(
  limit?: number,
  offset?: number,
  api: ApiClient = VaultAPI,
): Promise<BatchJobSummary[]> {
  const result = await api.listBatchJobs(limit, offset);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

export async function deleteBatchJob(jobId: string, api: ApiClient = VaultAPI): Promise<void> {
  const result = await api.deleteBatchJob(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
}

export async function retryFailedItems(jobId: string, itemId?: string, replacementPath?: string, api: ApiClient = VaultAPI): Promise<string> {
  const result = await api.retryFailedBatchItems(jobId, itemId, replacementPath);
  if (!result.ok) {
    const detail: unknown = result.details?.details;
    throw new Error(typeof detail === 'string' && detail.trim() ? detail : result.error);
  }
  return result.data;
}

export async function getBatchJobDetails(jobId: string, api: ApiClient = VaultAPI): Promise<BatchJobStatus> {
  const result = await api.getBatchJobStatus(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}
