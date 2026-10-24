import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { JOB_STATUS_EVENT } from '@/features/jobs/api';
import { deferred } from '@/tests/deferred';
import { jobFixture } from '@/tests/fixtures/jobs';

import { useIndexing } from '../useIndexing';

const api = vi.hoisted(() => ({ listBatchJobs: vi.fn(), getBatchJobStatus: vi.fn(), batchFileImport: vi.fn(), cancelBatchJob: vi.fn() }));
const events = vi.hoisted(() => new Map<string, Set<(event: { payload: unknown }) => void>>());
vi.mock('@/lib/api', () => ({ VaultAPI: api, default: api }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    const handlers = events.get(name) ?? new Set();
    handlers.add(handler);
    events.set(name, handlers);
    return Promise.resolve(() => handlers.delete(handler));
  }),
}));

const settle = async () => { await act(async () => { await vi.advanceTimersByTimeAsync(0); }); };
const report = async (id: string) => {
  await act(async () => {
    for (const handler of events.get(JOB_STATUS_EVENT) ?? []) handler({ payload: jobFixture({ id }) });
    await vi.advanceTimersByTimeAsync(0);
  });
};

const status = (value: string, extra: Record<string, unknown> = {}) => ({ ok: true, data: { status: value, totalItems: 2, completedItems: 1, failedItems: 0, items: [], ...extra } });

describe('useIndexing lifecycle', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    events.clear();
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('starts an import, tracks it from its first read, and forgets it on request', async () => {
    api.batchFileImport.mockResolvedValueOnce({ ok: true, data: 'job-1' });
    api.getBatchJobStatus.mockResolvedValueOnce(status('in_progress', { total_items: 3, totalItems: undefined, completed_items: 1, completedItems: undefined, failed_items: 1, failedItems: undefined, items: [{ url: 'https://example.org/a', status: 'RUNNING' }] }));
    const { result } = renderHook(() => useIndexing());
    await settle();
    let started!: unknown;
    await act(async () => { started = await result.current.startBatchImport(['/a.pdf', '/b.pdf'], 's-hist', { contextual: true } as never); });
    expect(started).toEqual({ ok: true, data: 'job-1' });
    expect(api.batchFileImport).toHaveBeenCalledWith(['/a.pdf', '/b.pdf'], 's-hist', { contextual: true });
    await settle();
    expect(result.current.getOperation('job-1')).toMatchObject({
      status: 'processing', totalFiles: 3, successfulFiles: 1, failedFiles: 1, processedFiles: 2, currentFile: 'https://example.org/a',
    });
    expect(result.current.isIndexing).toBe(true);
    act(() => result.current.clearOperation('job-1'));
    expect(result.current.getOperation('job-1')).toBeUndefined();
  });

  it('tracks nothing for a refused import', async () => {
    api.batchFileImport.mockResolvedValueOnce({ ok: false, error: 'no embedding model' });
    const { result } = renderHook(() => useIndexing());
    await settle();
    await act(async () => { await result.current.startBatchImport(['/a.pdf']); });
    expect(result.current.operations.size).toBe(0);
    expect(api.getBatchJobStatus).not.toHaveBeenCalled();
  });

  it.each([
    ['pending', 'pending'],
    ['success', 'completed'],
    ['error', 'error'],
    ['canceled', 'cancelled'],
    ['mystery', 'processing'],
  ])('maps a %s job to %s', async (backend, expected) => {
    api.batchFileImport.mockResolvedValueOnce({ ok: true, data: 'job-1' });
    api.getBatchJobStatus.mockResolvedValueOnce(status(backend));
    const { result } = renderHook(() => useIndexing());
    await settle();
    await act(async () => { await result.current.startBatchImport(['/a.pdf']); });
    await settle();
    expect(result.current.getOperation('job-1')?.status).toBe(expected);
  });

  it('counts a completed job with failures as an error', async () => {
    api.batchFileImport.mockResolvedValueOnce({ ok: true, data: 'job-1' });
    api.getBatchJobStatus.mockResolvedValueOnce(status('completed', { failedItems: undefined, failed_items: 1 }));
    const { result } = renderHook(() => useIndexing());
    await settle();
    await act(async () => { await result.current.startBatchImport(['/a.pdf']); });
    await settle();
    expect(result.current.getOperation('job-1')?.status).toBe('error');
  });

  it('reads a job again when it reports during a read, and retries a thrown read', async () => {
    const slow = deferred<unknown>();
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [{ id: 'job', jobType: 'file_import', status: 'running', totalItems: 2, completedItems: 0, failedItems: 0 }] });
    api.getBatchJobStatus.mockReturnValueOnce(slow.promise).mockRejectedValueOnce(new Error('ipc closed')).mockResolvedValue(status('running'));
    const { result } = renderHook(() => useIndexing());
    await settle();
    await report('job');
    expect(api.getBatchJobStatus).toHaveBeenCalledTimes(1);
    await act(async () => { slow.resolve(status('running')); await vi.advanceTimersByTimeAsync(0); });
    expect(api.getBatchJobStatus).toHaveBeenCalledTimes(2);
    expect(result.current.getOperation('job')?.error).toContain('ipc closed');
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(api.getBatchJobStatus).toHaveBeenCalledTimes(3);
    expect(result.current.getOperation('job')?.error).toBeUndefined();
  });

  it('skips jobs it already tracks, and finished jobs without failures', async () => {
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [
      { id: 'a', jobId: 'job-a', jobType: 'file_import', status: 'running', totalItems: 1, completedItems: 0, failedItems: 0 },
      { id: 'b', jobType: 'file_import', status: 'completed', totalItems: 1, completedItems: 1, failedItems: 0 },
      { id: 'c', jobType: 'url_import', status: 'running', totalItems: 1, completedItems: 0, failedItems: 0 },
    ] });
    api.getBatchJobStatus.mockResolvedValue(status('running'));
    const { result } = renderHook(() => useIndexing());
    await settle();
    expect([...result.current.operations.keys()]).toEqual(['job-a']);
  });

  it('says when import history cannot be read', async () => {
    api.listBatchJobs.mockRejectedValueOnce(new Error('history unavailable'));
    const { result } = renderHook(() => useIndexing());
    await settle();
    expect(result.current.historyError).toBeTruthy();
  });

  it('leaves a cancel that was refused alone, and ignores one for an unknown job', async () => {
    api.cancelBatchJob.mockResolvedValueOnce({ ok: false, error: 'already done' }).mockResolvedValueOnce({ ok: true, data: 0 });
    const { result } = renderHook(() => useIndexing());
    await settle();
    await act(async () => { await result.current.cancelBatchImport('job'); });
    await act(async () => { await result.current.cancelBatchImport('job'); });
    expect(result.current.operations.size).toBe(0);
  });

  it('stops retrying once unmounted', async () => {
    api.listBatchJobs.mockResolvedValue({ ok: true, data: [{ id: 'job', jobType: 'file_import', status: 'running', totalItems: 1, completedItems: 0, failedItems: 0 }] });
    api.getBatchJobStatus.mockResolvedValue({ ok: false, error: 'down' });
    const { unmount } = renderHook(() => useIndexing());
    await settle();
    unmount();
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    expect(api.getBatchJobStatus).toHaveBeenCalledTimes(1);
  });
});
