import { beforeEach, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type { BatchJobSummary } from '@/types/api/batch';

import { deleteBatchJob, getBatchJobDetails, listAllBatchJobs, listBatchJobs, retryFailedItems } from '../batchHistory';
import { cancelBatchJob, getBatchJobStatus, startBatchUrlImport } from '../batchImport';

vi.mock('@/lib/api', () => ({ VaultAPI: {
  listBatchJobs: vi.fn(), deleteBatchJob: vi.fn(), retryFailedBatchItems: vi.fn(),
  getBatchJobStatus: vi.fn(), cancelBatchJob: vi.fn(), startBatchUrlImport: vi.fn(),
} }));
beforeEach(() => { vi.resetAllMocks(); });

function jobs(count: number, offset = 0): BatchJobSummary[] {
  return Array.from({ length: count }, (_, index) => ({ id: `job-${offset + index}` } as BatchJobSummary));
}

it.each([0, 1, 99, 100, 101, 200, 237])('loads all %i imports, including failures older than the first page', async count => {
  const all = jobs(count);
  vi.mocked(VaultAPI.listBatchJobs).mockImplementation(async (limit = 100, offset = 0) => ({ ok: true, data: all.slice(offset, offset + limit) }));
  expect(await listAllBatchJobs()).toEqual(all);
  expect(VaultAPI.listBatchJobs).toHaveBeenCalledTimes(Math.floor(count / 100) + 1);
  for (let page = 0; page <= Math.floor(count / 100); page++) {
    expect(VaultAPI.listBatchJobs).toHaveBeenNthCalledWith(page + 1, 100, page * 100);
  }
});

it('rejects a failed later page instead of reporting an incomplete history as complete', async () => {
  vi.mocked(VaultAPI.listBatchJobs).mockResolvedValueOnce({ ok: true, data: jobs(100) }).mockResolvedValueOnce({ ok: false, error: 'Database unavailable' });
  await expect(listAllBatchJobs()).rejects.toThrow('Database unavailable');
});

it('forwards explicit pagination and optional import settings', async () => {
  vi.mocked(VaultAPI.listBatchJobs).mockResolvedValue({ ok: true, data: [] });
  vi.mocked(VaultAPI.startBatchUrlImport).mockResolvedValue({ ok: true, data: 'new-job' });
  await listBatchJobs(7, 13);
  expect(VaultAPI.listBatchJobs).toHaveBeenCalledWith(7, 13);
  const urls = ['https://example.org/café', 'https://example.org/two'];
  expect(await startBatchUrlImport(urls, { extractArticle: false })).toBe('new-job');
  expect(VaultAPI.startBatchUrlImport).toHaveBeenCalledWith(urls, { extractArticle: false });
});

it.each([
  ['deleteBatchJob', () => deleteBatchJob('job-1')],
  ['getBatchJobStatus', () => getBatchJobDetails('job-1')],
  ['getBatchJobStatus', () => getBatchJobStatus('job-1')],
  ['cancelBatchJob', () => cancelBatchJob('job-1')],
  ['startBatchUrlImport', () => startBatchUrlImport(['https://example.org'])],
] as const)('surfaces a backend %s failure', async (method, action) => {
  vi.mocked(VaultAPI[method]).mockResolvedValue({ ok: false, error: 'Disk is full' });
  await expect(action()).rejects.toThrow('Disk is full');
});

it('preserves successful cancellation counts, deletion and job details', async () => {
  const status = { jobId: 'job-1', status: 'cancelled' } as Awaited<ReturnType<typeof getBatchJobDetails>>;
  vi.mocked(VaultAPI.getBatchJobStatus).mockResolvedValue({ ok: true, data: status });
  vi.mocked(VaultAPI.cancelBatchJob).mockResolvedValue({ ok: true, data: 0 });
  vi.mocked(VaultAPI.deleteBatchJob).mockResolvedValue({ ok: true, data: undefined });
  expect(await getBatchJobDetails('job-1')).toBe(status);
  expect(await getBatchJobStatus('job-1')).toBe(status);
  expect(await cancelBatchJob('job-1')).toBe(0);
  await expect(deleteBatchJob('job-1')).resolves.toBeUndefined();
  expect(VaultAPI.deleteBatchJob).toHaveBeenCalledWith('job-1');
});

it.each([
  ['File was moved; select its new path', 'File was moved; select its new path'],
  ['', 'Retry failed'], ['   ', 'Retry failed'], [null, 'Retry failed'], [undefined, 'Retry failed'],
])('selects useful retry details from %j', async (details, message) => {
  vi.mocked(VaultAPI.retryFailedBatchItems).mockResolvedValue({ ok: false, error: 'Retry failed', details: { code: 'FILE_SYSTEM_ERROR', message: 'Retry failed', details } });
  await expect(retryFailedItems('job-1')).rejects.toThrow(message);
});

it('retries one missing item with its exact replacement path', async () => {
  vi.mocked(VaultAPI.retryFailedBatchItems).mockResolvedValue({ ok: true, data: 'retry-job' });
  expect(await retryFailedItems('old-job', 'item-2', '/new/資料.txt')).toBe('retry-job');
  expect(VaultAPI.retryFailedBatchItems).toHaveBeenCalledWith('old-job', 'item-2', '/new/資料.txt');
});
