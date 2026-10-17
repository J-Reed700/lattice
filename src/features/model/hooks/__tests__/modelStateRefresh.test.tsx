import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type { LoadedModelDto } from '@/lib/bindings';
import { listenValidated } from '@/types/events';

import { DEGRADED_RECHECK_MS, LOADED_MODELS_QUERY_KEY, degradedReasonFor, useLoadedModels } from '../useLoadedModels';
import { useModelWarmupListener } from '../useModelWarmupListener';

vi.mock('@/types/events', async (original) => ({
  ...(await original<typeof import('@/types/events')>()),
  listenValidated: vi.fn(),
}));

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;
const ornith = { kind: 'local', model_id: 'ornith-1.5-9b' } as const;

function wrapperFor(queryClient: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

function newClient() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

describe('model state that no event ends', () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
  afterEach(() => vi.useRealTimers());

  it('drops "on CPU" once the model that ran on the CPU is unloaded', async () => {
    const onCpu = { model: ornith, degraded: 'GPU init failed; running on the CPU' } as unknown as LoadedModelDto;
    api.listLoadedModels = vi
      .fn()
      .mockResolvedValueOnce({ ok: true, data: [onCpu] })
      .mockResolvedValue({ ok: true, data: [] });
    const { result } = renderHook(() => degradedReasonFor(ornith, useLoadedModels()), {
      wrapper: wrapperFor(newClient()),
    });
    await waitFor(() => expect(result.current).toBe('GPU init failed; running on the CPU'));

    await vi.advanceTimersByTimeAsync(DEGRADED_RECHECK_MS);

    await waitFor(() => expect(result.current).toBeNull());
  });
});

describe('a finished warm-up', () => {
  it('re-reads the loaded models', async () => {
    let deliver: ((event: { payload: unknown }) => void) | undefined;
    vi.mocked(listenValidated).mockImplementation(async (_name, _schema, handler) => {
      deliver = handler as typeof deliver;
      return () => undefined;
    });
    const queryClient = newClient();
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    renderHook(() => useModelWarmupListener(), { wrapper: wrapperFor(queryClient) });
    await waitFor(() => expect(deliver).toBeDefined());
    const invalidated = () => invalidate.mock.calls.map(([filters]) => filters?.queryKey);

    deliver?.({ payload: { role: 'learning', phase: 'started' } });
    expect(invalidated()).toEqual([]);

    deliver?.({ payload: { role: 'chat', phase: 'ready' } });
    expect(invalidated()).toEqual([LOADED_MODELS_QUERY_KEY]);
  });
});
