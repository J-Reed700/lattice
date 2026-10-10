import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { JOB_STATUS_EVENT } from '@/features/jobs/api';
import { jobFixture } from '@/tests/fixtures/jobs';

import { useIndexing } from '../useIndexing';

const api = vi.hoisted(() => ({ listBatchJobs: vi.fn(), getBatchJobStatus: vi.fn(), batchFileImport: vi.fn(), cancelBatchJob: vi.fn() }));
const events = vi.hoisted(() => new Map<string, Set<(event: { payload: unknown }) => void>>());
vi.mock('@/lib/api', () => ({ VaultAPI: api, default: api }));
vi.mock('../../lib/api', () => ({ VaultAPI: api, default: api }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    const handlers = events.get(name) ?? new Set();
    handlers.add(handler);
    events.set(name, handlers);
    return Promise.resolve(() => handlers.delete(handler));
  }),
}));

/** Lets pending reads settle. */
async function settle() {
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
}

/** The job runtime reports a change to `id`, as each settled file does. */
async function report(id: string) {
  await act(async () => {
    for (const handler of events.get(JOB_STATUS_EVENT) ?? []) handler({ payload: jobFixture({ id }) });
    await vi.advanceTimersByTimeAsync(0);
  });
}

describe('useIndexing progress', () => {
  beforeEach(() => { vi.useFakeTimers(); vi.clearAllMocks(); events.clear(); });
  afterEach(() => { vi.useRealTimers(); });
  it('reattaches to the 45-file import and tracks active files, successes and failures separately', async () => {
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [{ id: 'job', jobType: 'file_import', status: 'running', totalItems: 45, completedItems: 0, failedItems: 0 }] });
    api.getBatchJobStatus.mockResolvedValue({ ok: true, data: {
      status: 'running', totalItems: 45, completedItems: 2, failedItems: 1,
      items: [{ target: '/Downloads/chapter.pdf', status: 'running' }],
    } });
    const { result } = renderHook(() => useIndexing());
    await settle();
    expect(result.current.getOperation('job')).toMatchObject({
      totalFiles: 45, processedFiles: 3, successfulFiles: 2, failedFiles: 1,
      currentFile: '/Downloads/chapter.pdf', status: 'processing',
    });
    api.getBatchJobStatus.mockResolvedValue({ ok: true, data: { status: 'failed', totalItems: 45, completedItems: 44, failedItems: 1, items: [] } });
    await report('job');
    expect(result.current.getOperation('job')).toMatchObject({ processedFiles: 45, successfulFiles: 44, failedFiles: 1, status: 'error' });
    expect(result.current.isIndexing).toBe(false);
  });

  it('reads an import only when its own job reports, never on a timer', async () => {
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [{ id: 'job', jobType: 'file_import', status: 'running', totalItems: 2, completedItems: 0, failedItems: 0 }] });
    api.getBatchJobStatus.mockResolvedValue({ ok: true, data: { status: 'running', totalItems: 2, completedItems: 0, failedItems: 0, items: [] } });
    renderHook(() => useIndexing());
    await settle();
    expect(api.getBatchJobStatus).toHaveBeenCalledOnce();
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    await report('someone-else');
    expect(api.getBatchJobStatus).toHaveBeenCalledOnce();
    await report('job');
    expect(api.getBatchJobStatus).toHaveBeenCalledTimes(2);
  });

  it('restores a finished partial failure after the app restarts', async () => {
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [{ id: 'job', jobType: 'file_import', status: 'failed', totalItems: 45, completedItems: 44, failedItems: 1 }] });
    api.getBatchJobStatus.mockResolvedValue({ ok: true, data: {
      status: 'failed', totalItems: 45, completedItems: 44, failedItems: 1,
      items: [{ target: '/Downloads/mpep-2100.pdf', status: 'failed', errorMessage: 'PDF extraction timed out' }],
    } });
    const { result } = renderHook(() => useIndexing());
    await settle();
    expect(result.current.getOperation('job')).toMatchObject({
      status: 'error', successfulFiles: 44, failedFiles: 1,
      items: [expect.objectContaining({ errorMessage: 'PDF extraction timed out' })],
    });
    expect(result.current.isIndexing).toBe(false);
  });

  it('keeps checking after a status request fails instead of declaring the import failed', async () => {
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [{ id: 'job', jobType: 'file_import', status: 'running', totalItems: 1, completedItems: 0, failedItems: 0 }] });
    api.getBatchJobStatus.mockResolvedValue({ ok: false, error: 'Database unavailable' });
    const { result } = renderHook(() => useIndexing());
    await settle();
    expect(result.current.getOperation('job')).toMatchObject({ status: 'processing', failedFiles: 0, error: expect.stringContaining('Checking again') });
    api.getBatchJobStatus.mockResolvedValue({ ok: true, data: { status: 'completed', totalItems: 1, completedItems: 1, failedItems: 0, items: [] } });
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(result.current.getOperation('job')).toMatchObject({ status: 'completed', successfulFiles: 1, error: undefined });
  });

  it('cancels an active batch without inventing an outcome for the file still in flight', async () => {
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [{ id: 'job', jobType: 'file_import', status: 'running', totalItems: 2, completedItems: 0, failedItems: 0 }] });
    api.getBatchJobStatus.mockResolvedValue({ ok: true, data: {
      status: 'running', totalItems: 2, completedItems: 0, failedItems: 0,
      items: [{ target: '/one.pdf', status: 'processing' }, { target: '/two.pdf', status: 'pending' }],
    } });
    api.cancelBatchJob.mockResolvedValue({ ok: true, data: 1 });
    const { result } = renderHook(() => useIndexing());
    await settle();

    await act(async () => { await result.current.cancelBatchImport('job'); });

    expect(api.cancelBatchJob).toHaveBeenCalledWith('job');
    expect(result.current.getOperation('job')).toMatchObject({
      status: 'cancelled',
      items: [{ status: 'processing' }, { status: 'pending' }],
    });

    // That file finished indexing before the worker saw the cancellation; the
    // job's last report says so.
    api.getBatchJobStatus.mockResolvedValue({ ok: true, data: {
      status: 'cancelled', totalItems: 2, completedItems: 1, failedItems: 0,
      items: [{ target: '/one.pdf', status: 'completed' }, { target: '/two.pdf', status: 'cancelled' }],
    } });
    await report('job');
    expect(result.current.getOperation('job')).toMatchObject({
      status: 'cancelled', successfulFiles: 1,
      items: [{ status: 'completed' }, { status: 'cancelled' }],
    });
  });
});
