import { VaultAPI } from '@/lib/api';
import type { BatchJobStatus } from '@/types/api/batch';

export async function startBatchUrlImport(
  urls: string[],
  options?: { extractArticle?: boolean }
): Promise<string> {
  const result = await VaultAPI.startBatchUrlImport(urls, options);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

export async function getBatchJobStatus(jobId: string): Promise<BatchJobStatus> {
  const result = await VaultAPI.getBatchJobStatus(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

export async function cancelBatchJob(jobId: string): Promise<number> {
  const result = await VaultAPI.cancelBatchJob(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}
