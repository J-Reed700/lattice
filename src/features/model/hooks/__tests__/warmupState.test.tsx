import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import { listenValidated } from '@/types/events';

import { selectIsChatWarming, useModelWarmupStore } from '../../stores/modelWarmupStore';
import { useModelWarmupListener } from '../useModelWarmupListener';

vi.mock('@/types/events', async (original) => ({
  ...(await original<typeof import('@/types/events')>()),
  listenValidated: vi.fn(),
}));

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

type Deliver = (event: { payload: unknown }) => void;

function wrapperFor(queryClient: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

function mountListener(): { deliver: (payload: unknown) => void } {
  let handler: Deliver | undefined;
  vi.mocked(listenValidated).mockImplementation(async (_name, _schema, onEvent) => {
    handler = onEvent as Deliver;
    return () => undefined;
  });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  renderHook(() => useModelWarmupListener(), { wrapper: wrapperFor(queryClient) });
  return {
    deliver: (payload) => {
      act(() => handler?.({ payload }));
    },
  };
}

async function listening(deliver: (payload: unknown) => void) {
  await waitFor(() => expect(vi.mocked(listenValidated)).toHaveBeenCalled());
  return deliver;
}

describe('warm-up state follows the backend', () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    useModelWarmupStore.getState().reset();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.mocked(listenValidated).mockReset();
  });

  it('shows a warm-up that began before the window was listening', async () => {
    api.warmupStatus = vi.fn().mockResolvedValue({
      ok: true,
      data: [{ role: 'chat', phase: 'started', error: null, degraded: null, revision: 1 }],
    });

    mountListener();

    await waitFor(() => expect(selectIsChatWarming(useModelWarmupStore.getState())).toBe(true));
  });

  it('keeps a slow load warming until the backend says it ended', async () => {
    api.warmupStatus = vi.fn().mockResolvedValue({ ok: true, data: [] });
    const { deliver } = mountListener();
    await listening(deliver);

    deliver({ role: 'chat', phase: 'started', revision: 1 });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(120_000);
    });
    expect(selectIsChatWarming(useModelWarmupStore.getState())).toBe(true);

    deliver({ role: 'chat', phase: 'ready', revision: 2 });
    expect(selectIsChatWarming(useModelWarmupStore.getState())).toBe(false);
    expect(useModelWarmupStore.getState().chat.phase).toBe('ready');
  });

  it('keeps the newer warm-up status whichever way it arrived', async () => {
    api.warmupStatus = vi.fn().mockResolvedValue({
      ok: true,
      data: [{ role: 'chat', phase: 'ready', error: null, degraded: null, revision: 2 }],
    });
    const { deliver } = mountListener();
    await waitFor(() => expect(useModelWarmupStore.getState().chat.phase).toBe('ready'));

    deliver({ role: 'chat', phase: 'started', revision: 1 });

    expect(useModelWarmupStore.getState().chat.phase).toBe('ready');
    expect(selectIsChatWarming(useModelWarmupStore.getState())).toBe(false);
  });
});
