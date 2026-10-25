import { VaultAPI, type ApiClient } from '@/lib/api';
import type { BatchJobStatus } from '@/types/api/batch';

export async function startBatchUrlImport(
  urls: string[],
  options?: { extractArticle?: boolean },
  api: ApiClient = VaultAPI,
): Promise<string> {
  const result = await api.startBatchUrlImport(urls, options);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

export async function getBatchJobStatus(jobId: string, api: ApiClient = VaultAPI): Promise<BatchJobStatus> {
  const result = await api.getBatchJobStatus(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

export async function cancelBatchJob(jobId: string, api: ApiClient = VaultAPI): Promise<number> {
  const result = await api.cancelBatchJob(jobId);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}
