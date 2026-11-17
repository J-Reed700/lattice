import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import type { ActiveModels } from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';

import { useAdoptChatModel } from './useAdoptChatModel';

const served = { kind: 'llama_cpp', model: 'served.gguf' } as const;
const settings = makeAppSettings();
settings.llm.provider = 'llamacpp';
settings.llm.llamaCpp = { url: 'http://localhost:8080', model: 'served.gguf', authHeaderName: '' };

const answer = (patch: Partial<ActiveModels>): ActiveModels => ({
  chat: [], chat_unset: null, utility: [], llama_cpp: null, ollama: null, ollama_utility_tag: null, ...patch,
});

const api = {
  getSettings: vi.fn(),
  getDownloadedModels: vi.fn(),
  getActiveModels: vi.fn(),
  updateSettings: vi.fn(),
};

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={client}>
    <ApiProvider client={api as unknown as ApiClient}>{children}</ApiProvider>
  </QueryClientProvider>
);

describe('useAdoptChatModel', () => {
  beforeEach(() => {
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    api.getSettings.mockReset().mockResolvedValue({ ok: true, data: settings });
    api.getDownloadedModels.mockReset().mockResolvedValue({ ok: true, data: [] });
    api.getActiveModels.mockReset().mockResolvedValue({ ok: true, data: answer({ chat: [served], llama_cpp: served }) });
    api.updateSettings.mockReset().mockResolvedValue({ ok: true, data: settings });
  });

  it('offers the model the backend runs a default chat on, and sets Learning to it', async () => {
    const { result } = renderHook(() => useAdoptChatModel(settings.llm, []), { wrapper });
    await waitFor(() => expect(result.current.chatModel?.ref).toEqual(served));
    await act(() => result.current.adopt());
    expect(api.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { learningModel: served } });
  });

  it('offers nothing when the backend runs a default chat on nothing', async () => {
    api.getActiveModels.mockResolvedValue({
      ok: true, data: answer({ chat_unset: 'No llama.cpp model is set in Settings → Chat' }),
    });
    const { result } = renderHook(() => useAdoptChatModel(settings.llm, []), { wrapper });
    await waitFor(() => expect(api.getActiveModels).toHaveBeenCalled());
    expect(result.current.chatModel).toBeNull();
  });
});
