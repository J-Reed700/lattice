import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type { LearningWindowShortfallDto, LoadedModelDto } from '@/lib/bindings';
import { listenValidated } from '@/types/events';

import { MAX_WARMUP_AGE_MS, selectIsChatWarming, useModelWarmupStore } from '../../stores/modelWarmupStore';
import { LEARNING_WINDOW_SHORTFALL_QUERY_KEY, SHORTFALL_RECHECK_MS, useLearningWindowShortfall } from '../useLearningWindowShortfall';
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

  it('drops a Learning window shortfall once the backend no longer records it', async () => {
    const shortfall: LearningWindowShortfallDto = {
      model: ornith,
      needed_tokens: 20_000,
      window_tokens: 8_704,
      window_is_saved: true,
    };
    api.lastLearningWindowShortfall = vi
      .fn()
      .mockResolvedValueOnce({ ok: true, data: shortfall })
      .mockResolvedValue({ ok: true, data: null });
    const { result } = renderHook(() => useLearningWindowShortfall(), { wrapper: wrapperFor(newClient()) });
    await waitFor(() => expect(result.current).toEqual(shortfall));

    await vi.advanceTimersByTimeAsync(SHORTFALL_RECHECK_MS);

    await waitFor(() => expect(result.current).toBeNull());
    // Nothing is recorded, so nothing is polled.
    const reads = api.lastLearningWindowShortfall.mock.calls.length;
    await vi.advanceTimersByTimeAsync(SHORTFALL_RECHECK_MS * 3);
    expect(api.lastLearningWindowShortfall).toHaveBeenCalledTimes(reads);
  });

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
  it('re-reads the loaded models, and the shortfall when it was the Learning model', async () => {
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

    invalidate.mockClear();
    deliver?.({ payload: { role: 'learning', phase: 'failed', error: 'no memory' } });
    expect(invalidated()).toEqual([LOADED_MODELS_QUERY_KEY, LEARNING_WINDOW_SHORTFALL_QUERY_KEY]);
  });

  it('unblocks chat warmup when loaded models already show the chat model or after the safety timeout', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      useModelWarmupStore.getState().reset();
      vi.mocked(listenValidated).mockResolvedValue(() => undefined);
      const queryClient = newClient();
      renderHook(() => useModelWarmupListener(), { wrapper: wrapperFor(queryClient) });

      act(() => {
        useModelWarmupStore.getState().setRolePhase('chat', 'started', null, 'ornith-1.5-9b');
      });
      expect(selectIsChatWarming(useModelWarmupStore.getState())).toBe(true);

      // Reconcile via LOADED_MODELS_QUERY_KEY update (even if terminal Tauri event was dropped)
      act(() => {
        queryClient.setQueryData<LoadedModelDto[]>(LOADED_MODELS_QUERY_KEY, [
          {
            model: ornith,
            provider: 'local',
            context_tokens: 8192,
            in_flight: 0,
            pinned_by: ['chat'],
            loaded_at: new Date().toISOString(),
            last_used_at: new Date().toISOString(),
            gpu_bytes: null,
            degraded: null,
          } as LoadedModelDto,
        ]);
      });
      expect(useModelWarmupStore.getState().chat.phase).toBe('ready');
      expect(selectIsChatWarming(useModelWarmupStore.getState())).toBe(false);

      // Advance clock past the prior loaded_at timestamp before starting a new warmup
      // where neither event nor fresh loaded-model arrives within MAX_WARMUP_AGE_MS.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(10);
        useModelWarmupStore.getState().setRolePhase('chat', 'started', null, 'other-model');
      });
      expect(selectIsChatWarming(useModelWarmupStore.getState())).toBe(true);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(MAX_WARMUP_AGE_MS);
      });
      expect(useModelWarmupStore.getState().chat.phase).toBe('skipped');
      expect(selectIsChatWarming(useModelWarmupStore.getState())).toBe(false);
    } finally {
      vi.useRealTimers();
      useModelWarmupStore.getState().reset();
    }
  });
});
