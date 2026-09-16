import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LlmSettingsProvider } from './LlmSettingsProvider';
import { useLlmSettings } from './useLlmSettings';
import { VaultAPI } from '../../../lib/api';
import { makeAppSettings } from '../../../tests/fixtures/appSettings';

function wrapperWithClient() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>
        <LlmSettingsProvider>{children}</LlmSettingsProvider>
      </QueryClientProvider>
    );
  };
}

describe('useLlmSettings', () => {
  // The repository remembers writes, so the refetch after a mutation answers
  // with what was stored — the point of dropping the hand-rolled mirror.
  let storedTemperature = 0.7;

  beforeEach(() => {
    vi.clearAllMocks();
    storedTemperature = 0.7;
    vi.spyOn(VaultAPI, 'getSettings').mockImplementation(async () => {
      const settings = makeAppSettings();
      return { ok: true, data: { ...settings, llm: { ...settings.llm, temperature: storedTemperature } } };
    });
    vi.spyOn(VaultAPI, 'updateSettings').mockImplementation(async (payload) => {
      const updates = (payload as { updates: { temperature?: number } }).updates;
      if (typeof updates.temperature === 'number') {
        storedTemperature = updates.temperature;
      }
      const settings = makeAppSettings();
      return { ok: true, data: { ...settings, llm: { ...settings.llm, temperature: storedTemperature } } };
    });
  });

  it('reads the LLM block from the settings query', async () => {
    const { result } = renderHook(() => useLlmSettings(), { wrapper: wrapperWithClient() });

    await waitFor(() => expect(result.current.llmSettings).not.toBeNull());
    expect(result.current.llmSettings?.temperature).toBe(0.7);
    expect(VaultAPI.getSettings).toHaveBeenCalledTimes(1);
  });

  it('writes through the mutation, then re-reads the repository', async () => {
    const { result } = renderHook(() => useLlmSettings(), { wrapper: wrapperWithClient() });
    await waitFor(() => expect(result.current.llmSettings).not.toBeNull());

    let saved: boolean | undefined;
    await act(async () => {
      saved = await result.current.saveLlmUpdates({ temperature: 0.2 });
    });

    expect(saved).toBe(true);
    expect(VaultAPI.updateSettings).toHaveBeenCalledWith({
      category: 'llm',
      updates: { temperature: 0.2 },
    });
    // Invalidated, so the value on screen is the repository's, not the
    // response the mutation happened to return.
    await waitFor(() => expect(VaultAPI.getSettings).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current.llmSettings?.temperature).toBe(0.2));
  });

  it('reports a failed write and keeps showing what is stored', async () => {
    const { result } = renderHook(() => useLlmSettings(), { wrapper: wrapperWithClient() });
    await waitFor(() => expect(result.current.llmSettings).not.toBeNull());

    vi.spyOn(VaultAPI, 'updateSettings').mockResolvedValue({
      ok: false,
      error: 'settings.json is read-only',
    } as Awaited<ReturnType<typeof VaultAPI.updateSettings>>);

    let saved: boolean | undefined;
    await act(async () => {
      saved = await result.current.saveLlmUpdates({ temperature: 1.9 });
    });

    expect(saved).toBe(false);
    // No optimistic copy to roll back: the query still holds the stored value.
    expect(result.current.llmSettings?.temperature).toBe(0.7);
  });

  it('refetches on reload, which is what the Retry affordance calls', async () => {
    const { result } = renderHook(() => useLlmSettings(), { wrapper: wrapperWithClient() });
    await waitFor(() => expect(result.current.llmSettings).not.toBeNull());

    act(() => result.current.reload());

    await waitFor(() => expect(VaultAPI.getSettings).toHaveBeenCalledTimes(2));
  });
});
