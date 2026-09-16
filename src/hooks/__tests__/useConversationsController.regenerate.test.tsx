import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';
import { conversationUiStore } from '@/stores/conversationUiStore';

import { useConversationsController } from '../useConversationsController';


const regenerate = VaultAPI.regenerateResponse as unknown as ReturnType<typeof vi.fn>;

const wrapper = ({ children }: { children: ReactNode }) => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
};

const CONVERSATION_ID = 'conv-busy';

describe('regenerateResponse — busy guard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    conversationUiStore.setState({
      inFlightGenerations: new Map([[CONVERSATION_ID, 'request-in-flight']]),
      optimisticMessages: new Map(),
      composerDraft: null,
      error: null,
    });
  });

  afterEach(() => {
    conversationUiStore.setState({
      inFlightGenerations: new Map(),
      optimisticMessages: new Map(),
      composerDraft: null,
      error: null,
    });
  });

  it('reports "busy" rather than an answer when a turn is already running', async () => {
    // The guard used to return with no signal at all, so the caller announced
    // an answer that was never generated.
    const { result } = renderHook(() => useConversationsController(), { wrapper });

    const outcome = await result.current.regenerateResponse(CONVERSATION_ID);

    expect(outcome).toBe('busy');
    expect(regenerate).not.toHaveBeenCalled();
  });

  it('leaves the composer alone on the busy path', async () => {
    // Nothing was taken from the reader, so nothing is handed back — the
    // "your question is back in the composer" line would be a lie here.
    const { result } = renderHook(() => useConversationsController(), { wrapper });

    await result.current.regenerateResponse(CONVERSATION_ID);

    await waitFor(() =>
      expect(conversationUiStore.getState().composerDraft).toBeNull()
    );
    expect(
      conversationUiStore.getState().optimisticMessages.size
    ).toBe(0);
  });
});

describe('regenerateResponse — a turn that runs', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    conversationUiStore.setState({
      inFlightGenerations: new Map(),
      optimisticMessages: new Map(),
      composerDraft: null,
      error: null,
    });
  });

  it('reports "answered" when the backend returns a turn', async () => {
    regenerate.mockResolvedValue({
      ok: true,
      data: { conversationId: CONVERSATION_ID, messages: [], contextUsed: 0 },
    });
    const { result } = renderHook(() => useConversationsController(), { wrapper });

    const outcome = await result.current.regenerateResponse(CONVERSATION_ID);

    expect(outcome).toBe('answered');
    expect(regenerate).toHaveBeenCalledTimes(1);
  });

  it('reports "failed" and hands the question back when the backend refuses', async () => {
    regenerate.mockResolvedValue({ ok: false, error: 'no model loaded' });
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, gcTime: 0 } },
    });
    client.setQueryData(['conversations', 'messages', CONVERSATION_ID], [
      {
        id: 'm1',
        conversationId: CONVERSATION_ID,
        role: 'user',
        content: 'What did I decide about pricing?',
        createdAt: '2026-09-05T10:00:00Z',
      },
    ]);
    const { result } = renderHook(() => useConversationsController(), {
      wrapper: ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
      ),
    });

    const outcome = await result.current.regenerateResponse(CONVERSATION_ID);

    expect(outcome).toBe('failed');
    await waitFor(() =>
      expect(conversationUiStore.getState().composerDraft).toBe(
        'What did I decide about pricing?'
      )
    );
  });
});
