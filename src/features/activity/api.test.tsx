import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { ACTIVITY_REFRESH_EVENTS, useActivitySnapshot } from './api';

const events = vi.hoisted(() => ({
  handlers: new Map<string, () => void>(),
  unlisten: vi.fn(),
  failing: new Set<string>(),
  jobSubscriber: null as null | (() => void),
  stopJobs: vi.fn(),
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: events.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: () => void) => {
    if (events.failing.has(name)) return Promise.reject(new Error('no event bus'));
    events.handlers.set(name, handler);
    return Promise.resolve(events.unlisten);
  }),
}));
vi.mock('@/features/jobs/api', () => ({
  onJobStatus: (subscriber: () => void) => {
    events.jobSubscriber = subscriber;
    return events.stopJobs;
  },
}));

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return renderHook(() => useActivitySnapshot(), { wrapper });
}

const snapshot = (n: number) => ({ entries: [{ id: `job-${n}` }] });

beforeEach(() => {
  events.handlers.clear();
  events.failing.clear();
  events.unlisten.mockReset();
  events.stopJobs.mockReset();
  events.jobSubscriber = null;
  events.invoke.mockReset().mockResolvedValue(snapshot(1));
});

describe('useActivitySnapshot', () => {
  it('reads the snapshot on mount', async () => {
    const { result } = setup();
    await waitFor(() => expect(result.current.snapshot).toEqual(snapshot(1)));
    expect(events.invoke).toHaveBeenCalledWith('plugin:activity|activity_snapshot', {});
    expect(result.current.failed).toBe(false);
    expect(result.current.readAt).toBeGreaterThan(0);
  });

  it('reads again after each event that may move it, including job status', async () => {
    const { result } = setup();
    await waitFor(() => expect(result.current.snapshot).toEqual(snapshot(1)));
    await waitFor(() => expect([...events.handlers.keys()]).toEqual([...ACTIVITY_REFRESH_EVENTS]));

    events.invoke.mockResolvedValue(snapshot(2));
    act(() => events.handlers.get('download:completed')!());
    await waitFor(() => expect(result.current.snapshot).toEqual(snapshot(2)));

    events.invoke.mockResolvedValue(snapshot(3));
    act(() => events.jobSubscriber!());
    await waitFor(() => expect(result.current.snapshot).toEqual(snapshot(3)));
  });

  it('never overlaps reads: events during a read cause exactly one more read', async () => {
    const { result } = setup();
    await waitFor(() => expect(result.current.snapshot).toEqual(snapshot(1)));
    const first = deferred<unknown>();
    events.invoke.mockReset().mockReturnValueOnce(first.promise).mockResolvedValue(snapshot(3));

    let refreshed!: Promise<void>;
    act(() => { refreshed = result.current.refresh(); });
    act(() => {
      events.handlers.get('indexing://progress')!();
      events.handlers.get('activity://changed')!();
      void result.current.refresh();
    });
    expect(events.invoke).toHaveBeenCalledTimes(1);

    await act(async () => { first.resolve(snapshot(2)); await refreshed; });
    expect(events.invoke).toHaveBeenCalledTimes(2);
    await waitFor(() => expect(result.current.snapshot).toEqual(snapshot(3)));
  });

  it('reports a failed read', async () => {
    events.invoke.mockRejectedValue(new Error('scheduler unavailable'));
    const { result } = setup();
    await waitFor(() => expect(result.current.failed).toBe(true));
    expect(result.current.snapshot).toBeUndefined();
  });

  it('stops listening on unmount, tolerating events it could not attach to', async () => {
    events.failing.add('download:failed');
    const { result, unmount } = setup();
    await waitFor(() => expect(result.current.snapshot).toEqual(snapshot(1)));
    await waitFor(() => expect(events.handlers.size).toBe(ACTIVITY_REFRESH_EVENTS.length - 1));
    unmount();
    expect(events.stopJobs).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(events.unlisten).toHaveBeenCalledTimes(ACTIVITY_REFRESH_EVENTS.length - 1));
  });
});
