import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import { ConversationsProvider, useConversationsStore } from '@/stores/conversationsStore';
import { conversationUiStore } from '@/stores/conversationUiStore';
import { ErrorCode } from '@/types/api/errorCodes';

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

const conversation = {
  id: 'conversation-1',
  title: 'Test conversation',
  updatedAt: '2026-08-02T00:00:00.000Z',
};

const createWrapper = () => {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <ConversationsProvider>{children}</ConversationsProvider>
      </QueryClientProvider>
    );
  };
};

describe('useConversationsController optimistic cleanup', () => {
  beforeEach(() => {
    conversationUiStore.setState({
      selectedSpaceId: null,
      filterMode: 'all',
      searchQuery: '',
      activeConversationId: null,
      inFlightGenerations: new Map(),
      optimisticMessages: new Map(),
      error: null,
      requestedLinkedConversationIds: new Set(),
      requestedWebSourceConversationIds: new Set(),
      requestedMembershipDocumentIds: new Set(),
    });

    api.listConversationSpaces = vi.fn().mockResolvedValue({ ok: true, data: [] });
    api.listConversationsExplorer = vi.fn().mockResolvedValue({
      ok: true,
      data: { conversations: [conversation], total: 1 },
    });
    api.getConversationMessages = vi.fn().mockResolvedValue({
      ok: true,
      data: { messages: [], total: 0 },
    });
    api.cancelConversationGeneration = vi.fn().mockResolvedValue({ ok: true, data: undefined });
  });

  it('marks the user message failed and removes the assistant placeholder on transport errors', async () => {
    api.chatWithConversation = vi.fn().mockRejectedValue(new Error('transport unavailable'));
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    await act(async () => result.current.sendMessage('Hello'));

    expect(result.current.inFlightGenerations.size).toBe(0);
    expect(result.current.optimisticMessages.size).toBe(1);
    expect([...result.current.optimisticMessages.values()][0]).toMatchObject({
      role: 'user',
      status: 'failed',
      error: 'transport unavailable',
    });
  });

  it('removes both optimistic placeholders after a user cancellation', async () => {
    let resolveChat: ((value: unknown) => void) | undefined;
    api.chatWithConversation = vi.fn().mockImplementation(() => new Promise(resolve => {
      resolveChat = resolve;
    }));
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    let sendPromise: Promise<void> | undefined;
    act(() => {
      sendPromise = result.current.sendMessage('Stop this response');
    });
    await waitFor(() => expect(result.current.inFlightGenerations.size).toBe(1));

    await act(async () => result.current.cancelGeneration('conversation-1'));
    await act(async () => {
      resolveChat?.({
        ok: false,
        error: 'generation cancelled',
        details: { code: ErrorCode.INVALID_STATE },
      });
      await sendPromise;
    });

    expect(result.current.inFlightGenerations.size).toBe(0);
    expect(result.current.optimisticMessages.size).toBe(0);
    expect(result.current.error).toBeNull();
  });
});
