import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { JobDto } from '@/features/jobs/api';
import type { JournalSynthesisDto } from '@/lib/bindings';
import { jobFixture } from '@/tests/fixtures/jobs';

import { JOURNAL_SYNTHESIS_JOB, synthesisKeys, useJournalSyntheses } from './api';

const mocks = vi.hoisted(() => ({
  apiCall: vi.fn(),
  listen: vi.fn(),
  handlers: new Set<(event: { payload: unknown }) => void>(),
}));
vi.mock('@/shared/ipc/transport', () => ({ apiCall: mocks.apiCall }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));

function job(overrides: Partial<JobDto>): JobDto {
  return jobFixture({ id: 'synthesis', kind: JOURNAL_SYNTHESIS_JOB, subjectId: 'capture', progressTotal: 100, ...overrides });
}

function synthesis(jobDto: JobDto): JournalSynthesisDto {
  return {
    job: jobDto,
    title: 'Research discussion',
    heading: 'Research discussion',
    destination: { kind: 'capture' },
    conversationIds: ['chat'],
    activity: { stage: 'reading', entryCount: 3, chunkIndex: 1, chunkCount: 2 },
  };
}

function report(payload: unknown) {
  act(() => {
    for (const handler of mocks.handlers) handler({ payload });
  });
}

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

function cachedJob() {
  return client.getQueryData<JournalSynthesisDto[]>(synthesisKeys.list)?.[0]?.job;
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.handlers.clear();
  mocks.listen.mockImplementation((_name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.handlers.add(handler);
    return Promise.resolve(() => {
      mocks.handlers.delete(handler);
    });
  });
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
});

describe('the synthesis list', () => {
  it("keeps a synthesis's newer status when an older event arrives after it", async () => {
    mocks.apiCall.mockResolvedValue({ ok: true, data: [synthesis(job({ status: 'running', revision: 1 }))] });
    const { result } = renderHook(() => useJournalSyntheses(), { wrapper });
    await waitFor(() => expect(result.current.data).toHaveLength(1));
    await waitFor(() => expect(mocks.handlers.size).toBe(1));

    report(job({ status: 'completed', revision: 3, finishedAt: 3 }));
    report(job({ status: 'running', revision: 2 }));

    expect(cachedJob()).toMatchObject({ status: 'completed', revision: 3 });
  });

  it('keeps a finished synthesis finished when a list read from before it finished lands after its completion event', async () => {
    mocks.apiCall.mockResolvedValueOnce({ ok: true, data: [synthesis(job({ status: 'running', revision: 1 }))] });
    const { result } = renderHook(() => useJournalSyntheses(), { wrapper });
    await waitFor(() => expect(result.current.data).toHaveLength(1));
    await waitFor(() => expect(mocks.handlers.size).toBe(1));

    let release: (value: unknown) => void = () => {};
    mocks.apiCall.mockReturnValueOnce(new Promise((resolve) => { release = resolve; }));
    let refetch: Promise<void> = Promise.resolve();
    act(() => {
      refetch = client.invalidateQueries({ queryKey: synthesisKeys.list });
    });
    expect(mocks.apiCall).toHaveBeenCalledTimes(2);
    report(job({ status: 'completed', revision: 2, finishedAt: 2 }));
    await act(async () => {
      release({ ok: true, data: [synthesis(job({ status: 'running', revision: 1 }))] });
      await refetch;
    });

    expect(cachedJob()).toMatchObject({ status: 'completed', revision: 2 });
  });
});
