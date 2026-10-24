import { beforeEach, describe, expect, it } from 'vitest';

import { mockIPC } from '@/tests/setup';

import { batchApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

// A block body: a hook that returns the mock would register it as a teardown.
beforeEach(() => { mockIPC.mockReset(); });

describe('batchApi', () => {
  it('lists jobs with progress from completed and failed items', async () => {
    mockIPC.mockResolvedValue({
      jobs: [
        { jobId: 'j1', totalItems: 4, completedItems: 2, failedItems: 1 },
        { jobId: 'j2', totalItems: 0, completedItems: 0, failedItems: 0 },
        { jobId: 'j3', totalItems: 2, completedItems: 5, failedItems: 0 },
      ],
    });
    const result = await batchApi.listBatchJobs(10, 20);
    expect(lastCall()).toEqual(['plugin:batch|get_batch_history', { request: { limit: 10, offset: 20 } }]);
    expect(result).toEqual({
      ok: true,
      data: [
        expect.objectContaining({ id: 'j1', progress: 75 }),
        expect.objectContaining({ id: 'j2', progress: 0 }),
        expect.objectContaining({ id: 'j3', progress: 100 }),
      ],
    });
  });

  it('treats a missing job list as empty', async () => {
    mockIPC.mockResolvedValue({ jobs: null });
    await expect(batchApi.listBatchJobs()).resolves.toEqual({ ok: true, data: [] });
  });

  it('maps a job status and its items to the renderer shape', async () => {
    mockIPC.mockResolvedValue({
      jobId: 'j1', totalItems: 2, completedItems: 1, failedItems: 0, completedAt: undefined,
      items: [{ itemId: 'i1', target: 'https://example.org/bread', status: 'completed' }],
    });
    const result = await batchApi.getBatchJobStatus('j1');
    expect(lastCall()).toEqual(['plugin:batch|get_batch_status', { request: { jobId: 'j1' } }]);
    expect(result.ok && result.data).toMatchObject({
      id: 'j1', total_items: 2, completed_items: 1, failed_items: 0, progress: 50, completedAt: null,
      items: [{ id: 'i1', url: 'https://example.org/bread', status: 'completed' }],
    });
  });

  it('reports a delete the backend refused', async () => {
    mockIPC.mockResolvedValueOnce({ success: false });
    await expect(batchApi.deleteBatchJob('j1')).resolves.toEqual({ ok: false, error: 'Failed to delete batch job' });
    mockIPC.mockResolvedValueOnce({ success: true });
    await expect(batchApi.deleteBatchJob('j1')).resolves.toEqual({ ok: true, data: undefined });
    expect(lastCall()).toEqual(['plugin:batch|delete_batch_job', { jobId: 'j1' }]);
  });

  it('returns the ids and counts the backend hands back', async () => {
    mockIPC.mockResolvedValueOnce({ newJobId: 'j9' });
    await expect(batchApi.retryFailedBatchItems('j1', 'i2', '/notes/fixed.md')).resolves.toEqual({ ok: true, data: 'j9' });
    expect(lastCall()[1]).toEqual({ jobId: 'j1', itemId: 'i2', replacementPath: '/notes/fixed.md' });

    mockIPC.mockResolvedValueOnce({ cancelledCount: 3 });
    await expect(batchApi.cancelBatchJob('j1')).resolves.toEqual({ ok: true, data: 3 });

    mockIPC.mockResolvedValueOnce({ jobId: 'files-1' });
    await expect(batchApi.startBatchFileImport(['/a.md'], 'chat-7')).resolves.toEqual({ ok: true, data: 'files-1' });
    expect(lastCall()[1]).toEqual({ request: { filePaths: ['/a.md'], ownerConversationId: 'chat-7' } });

    mockIPC.mockResolvedValueOnce({ jobId: 'urls-1' });
    await expect(batchApi.startBatchUrlImport(['https://example.org'], { extractArticle: true })).resolves.toEqual({ ok: true, data: 'urls-1' });
    expect(lastCall()[1]).toEqual({ request: { urls: ['https://example.org'], options: { extractArticle: true } } });

    mockIPC.mockResolvedValueOnce({ jobId: 'urls-2' });
    await batchApi.startBatchUrlImport(['https://example.org']);
    expect(lastCall()[1]).toEqual({ request: { urls: ['https://example.org'], options: null } });

    mockIPC.mockResolvedValueOnce({ jobId: 'files-2' });
    await expect(batchApi.batchFileImport(['/b.md'], 'space-1', { chunkSize: 400 } as never)).resolves.toEqual({ ok: true, data: 'files-2' });
    expect(lastCall()[1]).toEqual({ request: { filePaths: ['/b.md'], spaceId: 'space-1', indexing: { chunkSize: 400 } } });
  });

  it('passes a backend failure through from every call', async () => {
    mockIPC.mockRejectedValue(new Error('queue is closed'));
    const calls = [
      batchApi.listBatchJobs(),
      batchApi.deleteBatchJob('j1'),
      batchApi.retryFailedBatchItems('j1'),
      batchApi.getBatchJobStatus('j1'),
      batchApi.cancelBatchJob('j1'),
      batchApi.startBatchFileImport([]),
      batchApi.startBatchUrlImport([]),
      batchApi.batchFileImport([]),
    ];
    for (const result of await Promise.all(calls)) {
      expect(result).toMatchObject({ ok: false, error: 'queue is closed' });
    }
  });
});
