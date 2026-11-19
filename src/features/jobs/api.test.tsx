import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { jobFixture } from '@/tests/fixtures/jobs';

import { JOB_STATUS_EVENT, onJobStatus, upsertJob, useJobs } from './api';

const mocks = vi.hoisted(() => ({
  apiCall: vi.fn(),
  listen: vi.fn(),
  handlers: new Set<(event: { payload: unknown }) => void>(),
  unlisten: vi.fn(),
}));
vi.mock('@/shared/ipc/transport', () => ({ apiCall: mocks.apiCall }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));

function report(payload: unknown) {
  act(() => {
    for (const handler of mocks.handlers) handler({ payload });
  });
}

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

beforeEach(() => {
  vi.clearAllMocks();
  mocks.handlers.clear();
  mocks.listen.mockImplementation((_name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.handlers.add(handler);
    return Promise.resolve(() => {
      mocks.handlers.delete(handler);
      mocks.unlisten();
    });
  });
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
});

describe('job status reports', () => {
  it('share one window listener, removed once nothing is subscribed', async () => {
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = onJobStatus(first);
    const stopSecond = onJobStatus(second);
    await waitFor(() => expect(mocks.handlers.size).toBe(1));
    expect(mocks.listen).toHaveBeenCalledOnce();
    expect(mocks.listen).toHaveBeenCalledWith(JOB_STATUS_EVENT, expect.any(Function));

    report(jobFixture({ id: 'a' }));
    expect(first).toHaveBeenCalledWith(expect.objectContaining({ id: 'a' }));
    expect(second).toHaveBeenCalledWith(expect.objectContaining({ id: 'a' }));

    stopFirst();
    report(jobFixture({ id: 'b' }));
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(2);
    expect(mocks.unlisten).not.toHaveBeenCalled();
    stopSecond();
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledOnce());
  });

  it('keep the jobs of the asked kinds current after their first read', async () => {
    mocks.apiCall.mockResolvedValue({ ok: true, data: [jobFixture({ id: 'import', kind: 'batch.file_import' })] });
    const { result } = renderHook(() => useJobs(['batch.file_import']), { wrapper });
    await waitFor(() => expect(result.current.data).toHaveLength(1));
    expect(mocks.apiCall).toHaveBeenCalledWith('list_jobs', { kinds: ['batch.file_import'] });

    report(jobFixture({ id: 'import', kind: 'batch.file_import', status: 'completed', finishedAt: 2 }));
    report(jobFixture({ id: 'build', kind: 'explorer.folder_index' }));
    report(jobFixture({ id: 'second', kind: 'batch.file_import' }));
    await waitFor(() => expect(result.current.data?.map((job) => [job.id, job.status])).toEqual([
      ['import', 'completed'],
      ['second', 'running'],
    ]));
  });

  it('recovers the shared listener if listen() fails', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    mocks.listen.mockRejectedValueOnce(new Error('bus offline'));
    const stopFailed = onJobStatus(vi.fn());
    await waitFor(() => expect(mocks.listen).toHaveBeenCalledTimes(1));
    stopFailed();

    const next = vi.fn();
    const stopNext = onJobStatus(next);
    await waitFor(() => expect(mocks.listen).toHaveBeenCalledTimes(2));
    report(jobFixture({ id: 'recovered' }));
    expect(next).toHaveBeenCalledWith(expect.objectContaining({ id: 'recovered' }));
    stopNext();
    error.mockRestore();
  });

  it('never regresses terminal or higher-progress jobs on out-of-order events or late snapshots', async () => {
    const completed = jobFixture({ id: 'j1', status: 'completed', finishedAt: 10, progressCurrent: 10 });
    const regressed = upsertJob([completed], jobFixture({ id: 'j1', status: 'running', progressCurrent: 2 }));
    expect(regressed[0]?.status).toBe('completed');

    const failed = jobFixture({ id: 'j2', status: 'failed', finishedAt: 10 });
    expect(upsertJob([failed], jobFixture({ id: 'j2', status: 'pending' }))[0]?.status).toBe('failed');

    const cancelled = jobFixture({ id: 'j3', status: 'cancelled', finishedAt: 10 });
    expect(upsertJob([cancelled], jobFixture({ id: 'j3', status: 'running' }))[0]?.status).toBe('cancelled');

    const running5 = jobFixture({ id: 'j4', status: 'running', startedAt: 5, progressCurrent: 5 });
    expect(upsertJob([running5], jobFixture({ id: 'j4', status: 'running', startedAt: 5, progressCurrent: 2 }))[0]?.progressCurrent).toBe(5);
    expect(upsertJob([running5], jobFixture({ id: 'j4', status: 'pending', retryCount: 0 }))[0]?.status).toBe('running');

    // A deferred retry transitions running (retryCount: 0) -> pending (retryCount: 1).
    const deferred = upsertJob(
      [jobFixture({ id: 'j-retry', status: 'running', retryCount: 0 })],
      jobFixture({ id: 'j-retry', status: 'pending', retryCount: 1, retryNotBefore: 12345 }),
    );
    expect(deferred[0]).toMatchObject({ id: 'j-retry', status: 'pending', retryCount: 1, retryNotBefore: 12345 });
    // Once deferred to retryCount: 1, a stale running (retryCount: 0) snapshot cannot overwrite it.
    expect(upsertJob(deferred, jobFixture({ id: 'j-retry', status: 'running', retryCount: 0 }))[0]?.retryCount).toBe(1);

    // Simulate an in-flight list_jobs snapshot returning running while a live completed event lands first.
    let resolveList!: (value: { ok: true; data: unknown[] }) => void;
    mocks.apiCall.mockReturnValueOnce(new Promise((resolve) => { resolveList = resolve; }));
    const { result } = renderHook(() => useJobs(['batch.file_import']), { wrapper });
    await waitFor(() => expect(mocks.handlers.size).toBe(1));

    report(jobFixture({ id: 'snap-1', kind: 'batch.file_import', status: 'completed', finishedAt: 20 }));
    await act(async () => {
      resolveList({ ok: true, data: [jobFixture({ id: 'snap-1', kind: 'batch.file_import', status: 'running', progressCurrent: 1 })] });
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([
      expect.objectContaining({ id: 'snap-1', status: 'completed', finishedAt: 20 }),
    ]);
  });

  it('keeps all active jobs and caps retained terminal jobs at 100', () => {
    let jobs = [jobFixture({ id: 'active-1', status: 'running' })];
    for (let i = 0; i < 120; i += 1) {
      jobs = upsertJob(jobs, jobFixture({ id: `done-${i}`, status: 'completed', finishedAt: 1000 + i }));
    }
    const active = jobs.filter((j) => j.status === 'running');
    const finished = jobs.filter((j) => j.status === 'completed');
    expect(active).toHaveLength(1);
    expect(finished).toHaveLength(100);
    expect(finished.some((j) => j.id === 'done-119')).toBe(true);
    expect(finished.some((j) => j.id === 'done-0')).toBe(false);
  });
});
