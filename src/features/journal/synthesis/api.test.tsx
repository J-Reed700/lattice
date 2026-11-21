import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { JobDto } from '@/features/jobs/api';
import type { JournalSynthesisDto } from '@/lib/bindings';
import { jobFixture } from '@/tests/fixtures/jobs';

import { JOURNAL_SYNTHESIS_JOB, mergeSyntheses, rememberSynthesis, synthesisKeys, useJournalSyntheses } from './api';

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

describe('rememberSynthesis', () => {
  it('preserves a newer cached job when a late pending start or retry response arrives, places it first, and removes any superseded retryOfJobId entry', () => {
    const writingActivity = { stage: 'writing', entryCount: 3, chunkIndex: 2, chunkCount: 2 } as const;
    const other = synthesis(job({ id: 'other', status: 'completed', revision: 3, finishedAt: 3 }));
    const superseded = synthesis(job({ id: 'failed-1', status: 'failed', revision: 1, finishedAt: 1 }));
    const newerCached = {
      ...synthesis(job({ id: 'retry-2', retryOfJobId: 'failed-1', status: 'running', revision: 2, activity: writingActivity })),
      activity: writingActivity,
    };
    const lateRetryResponse = {
      ...synthesis(job({ id: 'retry-2', retryOfJobId: 'failed-1', status: 'pending', revision: 0, activity: null })),
      activity: null,
    };

    const next = rememberSynthesis([other, superseded, newerCached], lateRetryResponse);

    expect(next.map((item) => item.job.id)).toEqual(['retry-2', 'other']);
    expect(next[0].job).toEqual(newerCached.job);
    expect(next[0].activity).toEqual(writingActivity);
  });

  it('preserves a completed cached job over a late revision 0 response', () => {
    const completed = synthesis(job({ id: 'synth-1', status: 'completed', revision: 2, finishedAt: 4 }));
    const lateStartResponse = synthesis(job({ id: 'synth-1', status: 'pending', revision: 0 }));

    const next = rememberSynthesis([completed], lateStartResponse);

    expect(next).toHaveLength(1);
    expect(next[0].job).toEqual(completed.job);
  });
});

describe('mergeSyntheses', () => {
  it('returns the snapshot item directly when cached and snapshot revisions are equal', () => {
    const cachedItem = synthesis(job({ id: 'synth-1', status: 'running', revision: 1 }));
    const snapshotItem = synthesis(job({ id: 'synth-1', status: 'running', revision: 1 }));

    const merged = mergeSyntheses([cachedItem], [snapshotItem]);

    expect(merged).toHaveLength(1);
    expect(merged[0]).toBe(snapshotItem);
  });

  it('preserves the cached job when the cached revision is higher than the snapshot revision', () => {
    const cachedJob = job({ id: 'synth-1', status: 'completed', revision: 3, finishedAt: 5 });
    const cachedItem = synthesis(cachedJob);
    const snapshotItem = synthesis(job({ id: 'synth-1', status: 'running', revision: 1 }));

    const merged = mergeSyntheses([cachedItem], [snapshotItem]);

    expect(merged).toHaveLength(1);
    expect(merged[0].job).toBe(cachedJob);
  });
});
