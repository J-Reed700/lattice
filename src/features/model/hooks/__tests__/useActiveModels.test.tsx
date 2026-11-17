import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import { SETTINGS_QUERY_KEY } from '@/features/settings/hooks/useSettingsQuery';
import type { ActiveModels } from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';

import { useActiveModels } from '../useActiveModels';

const answer = (model: string): ActiveModels => ({
  chat: [{ kind: 'llama_cpp', model }], chat_unset: null, utility: [], llama_cpp: null, ollama: null, ollama_utility_tag: null,
});

const api = {
  getSettings: vi.fn(),
  getDownloadedModels: vi.fn(),
  getActiveModels: vi.fn(),
};

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={client}>
    <ApiProvider client={api as unknown as ApiClient}>{children}</ApiProvider>
  </QueryClientProvider>
);

describe('useActiveModels', () => {
  beforeEach(() => {
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    api.getSettings.mockReset().mockResolvedValue({ ok: true, data: makeAppSettings() });
    api.getDownloadedModels.mockReset().mockResolvedValue({ ok: true, data: [] });
    api.getActiveModels.mockReset().mockResolvedValue({ ok: true, data: answer('first.gguf') });
  });

  it('asks the backend once Settings and the rows have loaded, and returns its answer', async () => {
    const { result } = renderHook(() => useActiveModels(), { wrapper });
    expect(result.current.activeModels).toBeNull();
    await waitFor(() => expect(result.current.activeModels).toEqual(answer('first.gguf')));
    expect(api.getActiveModels).toHaveBeenCalledTimes(1);
  });

  it('asks again when Settings change, and keeps the last answer until the new one arrives', async () => {
    const { result } = renderHook(() => useActiveModels(), { wrapper });
    await waitFor(() => expect(result.current.activeModels).toEqual(answer('first.gguf')));

    let release: (value: { ok: true; data: ActiveModels }) => void = () => undefined;
    api.getActiveModels.mockReturnValueOnce(new Promise((resolve) => { release = resolve; }));
    act(() => {
      client.setQueryData(SETTINGS_QUERY_KEY, { ...makeAppSettings(), updatedMarker: 1 });
    });
    await waitFor(() => expect(api.getActiveModels).toHaveBeenCalledTimes(2));
    expect(result.current.activeModels).toEqual(answer('first.gguf'));
    await act(async () => { release({ ok: true, data: answer('second.gguf') }); });
    await waitFor(() => expect(result.current.activeModels).toEqual(answer('second.gguf')));
  });

  it('reports a refused question as an error', async () => {
    api.getActiveModels.mockResolvedValue({ ok: false, error: 'no settings' });
    const { result } = renderHook(() => useActiveModels(), { wrapper });
    await waitFor(() => expect(result.current.error?.message).toBe('no settings'));
    expect(result.current.activeModels).toBeNull();
  });
});
