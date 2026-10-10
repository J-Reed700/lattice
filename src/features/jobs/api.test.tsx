import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { jobFixture } from '@/tests/fixtures/jobs';

import { JOB_STATUS_EVENT, onJobStatus, useJobs } from './api';

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
});
