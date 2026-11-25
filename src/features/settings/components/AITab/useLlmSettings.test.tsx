import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, renderHook, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import { VaultAPI, type ApiClient } from '@/lib/api';
import { createTestApi } from '@/tests/apiClient';
import { deferred } from '@/tests/deferred';
import { makeAppSettings } from '@/tests/fixtures/appSettings';

import { ChatTab } from './ChatTab';
import { LlmSettingsProvider } from './LlmSettingsProvider';
import { useLlmSettings } from './useLlmSettings';

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

  describe('remote auth status states without mocking useLlmSettings', () => {
    const withSavedAuthHeaders = () => {
      const settings = makeAppSettings();
      return {
        ok: true as const,
        data: {
          ...settings,
          llm: {
            ...settings.llm,
            provider: 'auto' as const,
            ollamaAuthHeaderName: 'Authorization',
            llamaCpp: {
              url: 'http://localhost:8080',
              model: 'qwen',
              authHeaderName: 'X-Llama-Key',
            },
          },
        },
      };
    };

    function wrapperWithApi(overrides: Partial<ApiClient>) {
      const client = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const testApi = createTestApi({
        getSettings: vi.fn().mockResolvedValue(withSavedAuthHeaders()),
        getDownloadedModels: vi.fn().mockResolvedValue({ ok: true, data: [] }),
        getActiveModels: vi.fn().mockResolvedValue({
          ok: true,
          data: { chat: [], chat_unset: null, utility: [], llama_cpp: null, ollama: null, ollama_utility_tag: null },
        }),
        ...overrides,
      });
      return function Wrapper({ children }: { children: ReactNode }) {
        return (
          <ApiProvider client={testApi}>
            <QueryClientProvider client={client}>
              <LlmSettingsProvider>{children}</LlmSettingsProvider>
            </QueryClientProvider>
          </ApiProvider>
        );
      };
    }

    it('treats loading Keychain status as loading, not missing', async () => {
      const pending = deferred<{ ok: true; data: { ollama: boolean; llamaCpp: boolean } }>();
      const Wrapper = wrapperWithApi({
        getRemoteAuthHeaderStatus: vi.fn().mockReturnValue(pending.promise),
      });

      const { result } = renderHook(() => useLlmSettings(), { wrapper: Wrapper });
      await waitFor(() => expect(result.current.llmSettings).not.toBeNull());
      expect(result.current.authValueStatus).toBe('loading');
      expect(result.current.authValueSet).toEqual({ ollama: false, llamaCpp: false });

      render(
        <Wrapper>
          <ChatTab />
        </Wrapper>,
      );
      await waitFor(() => expect(screen.getByLabelText('llama.cpp auth header name')).toHaveValue('X-Llama-Key'));
      expect(screen.queryByText(/without a value in Keychain/)).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Clear header' })).not.toBeInTheDocument();
      expect(screen.getAllByText('Checking Keychain for the saved header value…')).toHaveLength(2);
    });

    it('treats errored Keychain status as error, not missing', async () => {
      const Wrapper = wrapperWithApi({
        getRemoteAuthHeaderStatus: vi.fn().mockResolvedValue({
          ok: false,
          error: 'keychain locked',
        }),
      });

      const { result } = renderHook(() => useLlmSettings(), { wrapper: Wrapper });
      await waitFor(() => expect(result.current.authValueStatus).toBe('error'));
      expect(result.current.authValueSet).toEqual({ ollama: false, llamaCpp: false });

      render(
        <Wrapper>
          <ChatTab />
        </Wrapper>,
      );
      await waitFor(() =>
        expect(screen.getAllByText('Couldn’t check Keychain status. Saved header settings are preserved.')).toHaveLength(2),
      );
      expect(screen.queryByText(/without a value in Keychain/)).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Clear header' })).not.toBeInTheDocument();
    });

    it('reports stored status cleanly when Keychain confirms values exist', async () => {
      const Wrapper = wrapperWithApi({
        getRemoteAuthHeaderStatus: vi.fn().mockResolvedValue({
          ok: true,
          data: { ollama: true, llamaCpp: true },
        }),
      });

      const { result } = renderHook(() => useLlmSettings(), { wrapper: Wrapper });
      await waitFor(() => expect(result.current.authValueStatus).toBe('ready'));
      expect(result.current.authValueSet).toEqual({ ollama: true, llamaCpp: true });

      render(
        <Wrapper>
          <ChatTab />
        </Wrapper>,
      );
      await waitFor(() => expect(screen.getAllByRole('button', { name: 'Clear header' })).toHaveLength(2));
      expect(screen.queryByText(/without a value in Keychain/)).not.toBeInTheDocument();
      expect(screen.getAllByPlaceholderText('Value is set. Type to replace it')).toHaveLength(2);
    });

    it('shows the missing-value warning only when Keychain confirms the secret is absent', async () => {
      const Wrapper = wrapperWithApi({
        getRemoteAuthHeaderStatus: vi.fn().mockResolvedValue({
          ok: true,
          data: { ollama: false, llamaCpp: false },
        }),
      });

      const { result } = renderHook(() => useLlmSettings(), { wrapper: Wrapper });
      await waitFor(() => expect(result.current.authValueStatus).toBe('ready'));
      expect(result.current.authValueSet).toEqual({ ollama: false, llamaCpp: false });

      render(
        <Wrapper>
          <ChatTab />
        </Wrapper>,
      );
      await waitFor(() =>
        expect(screen.getByText('Auth header name is saved without a value in Keychain. Enter a value or click Clear header.')).toBeInTheDocument(),
      );
      expect(screen.getByText('Header name is saved without a value in Keychain. Enter a value or click Clear header.')).toBeInTheDocument();
      expect(screen.getAllByRole('button', { name: 'Clear header' })).toHaveLength(2);
    });
  });
});
