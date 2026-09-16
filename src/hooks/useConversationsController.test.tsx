import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useSidebarBookmarksQuery } from '@/components/Chat/sidebar/workspaceQueries';
import { VaultAPI } from '@/lib/api';
import { ConversationsProvider, useConversationsStore } from '@/stores/conversationsStore';
import { conversationUiStore } from '@/stores/conversationUiStore';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
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

  it('creates a llama.cpp conversation without a downloaded local model', async () => {
    const settings = makeAppSettings();
    settings.llm.provider = 'llamacpp';
    settings.llm.llamaCpp.model = 'qwen.gguf';
    api.getActiveModels = vi.fn().mockResolvedValue({ ok: true, data: { chat_model: null } });
    api.getSettings = vi.fn().mockResolvedValue({ ok: true, data: settings });
    api.createConversation = vi.fn().mockResolvedValue({ ok: true, data: { conversation: { ...conversation, id: 'remote-conversation' } } });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await act(async () => { await result.current.createConversation('Remote chat'); });
    expect(api.createConversation).toHaveBeenCalledWith('Remote chat', 'qwen.gguf');
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

  it('opens a new chat only after verifying its saved selected space', async () => {
    const selected = { id: 'patent', name: 'Patent Training', defaultModelName: 'qwen.gguf' };
    conversationUiStore.setState({ selectedSpaceId: selected.id });
    api.listConversationSpaces = vi.fn().mockResolvedValue({ ok: true, data: [selected] });
    api.getActiveModels = vi.fn().mockResolvedValue({ ok: true, data: { chat_model: null } });
    api.getSettings = vi.fn().mockResolvedValue({ ok: true, data: makeAppSettings() });
    const created = { ...conversation, id: 'new-chat', spaceId: 'space_general' };
    api.createConversation = vi.fn().mockResolvedValue({ ok: true, data: { conversation: created } });
    api.moveConversationToSpace = vi.fn().mockResolvedValue({ ok: true, data: {} });
    api.getConversation = vi.fn().mockResolvedValue({ ok: true, data: { conversation: { ...created, spaceId: selected.id } } });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await act(async () => { await result.current.createConversation('Training'); });
    expect(api.moveConversationToSpace).toHaveBeenCalledWith({ conversationId: 'new-chat', spaceId: 'patent' });
    expect(result.current.activeConversationId).toBe('new-chat');
    expect(result.current.conversations.find(c => c.id === 'new-chat')?.spaceId).toBe('patent');
  });

  it('surfaces a failed space assignment without opening the General chat', async () => {
    conversationUiStore.setState({ selectedSpaceId: 'patent' });
    api.listConversationSpaces = vi.fn().mockResolvedValue({ ok: true, data: [{ id: 'patent', name: 'Patent Training', defaultModelName: 'qwen.gguf' }] });
    api.getActiveModels = vi.fn().mockResolvedValue({ ok: true, data: { chat_model: null } });
    api.getSettings = vi.fn().mockResolvedValue({ ok: true, data: makeAppSettings() });
    api.createConversation = vi.fn().mockResolvedValue({ ok: true, data: { conversation: { ...conversation, id: 'new-chat', spaceId: 'space_general' } } });
    api.moveConversationToSpace = vi.fn().mockResolvedValue({ ok: false, error: 'database unavailable' });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await act(async () => {
      await expect(result.current.createConversation('Training')).rejects.toThrow('Could not create the chat in Patent Training');
    });
    expect(result.current.activeConversationId).not.toBe('new-chat');
    expect(result.current.error).toContain('database unavailable');
  });

  it('shows the provider failure details and preserves the failed prompt for retry', async () => {
    api.chatWithConversation = vi.fn().mockResolvedValue({
      ok: false,
      error: 'Network error',
      details: { code: ErrorCode.NETWORK_ERROR, details: 'llama.cpp request timed out' },
    });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    await act(async () => result.current.sendMessage('Help me learn the MPEP'));
    expect(result.current.error).toBe('llama.cpp request timed out');
    expect(result.current.inFlightGenerations.size).toBe(0);
    expect([...result.current.optimisticMessages.values()]).toEqual([
      expect.objectContaining({ content: 'Help me learn the MPEP', role: 'user', status: 'failed', error: 'llama.cpp request timed out' }),
    ]);
  });

  it('replaces a failed partial draft after a retry and ignores another request', async () => {
    let emit: ((event: { payload: Record<string, unknown> }) => void) | undefined;
    vi.mocked(listen).mockImplementationOnce(async (_event, handler) => {
      emit = handler as typeof emit;
      return () => {};
    });
    let resolveChat: ((value: unknown) => void) | undefined;
    api.chatWithConversation = vi.fn().mockImplementation(() => new Promise(resolve => { resolveChat = resolve; }));
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    let sending: Promise<void> | undefined;
    act(() => { sending = result.current.sendMessage('Try again'); });
    await waitFor(() => expect(api.chatWithConversation).toHaveBeenCalled());
    const requestId = api.chatWithConversation.mock.calls[0][3];
    const send = (payload: Record<string, unknown>) => act(() => emit?.({ payload: {
      conversationId: 'conversation-1', requestId, done: false, content: '', ...payload,
    } }));
    const draft = () => [...result.current.optimisticMessages.values()].find(message => message.role === 'assistant')?.content;
    send({ content: 'Failed partial answer' });
    expect(draft()).toBe('Failed partial answer');
    send({ status: 'retrying', attempt: 2, requestId: 'unrelated' });
    expect(draft()).toBe('Failed partial answer');
    send({ status: 'retrying', attempt: 2 });
    expect(draft()).toContain('Retrying');
    send({ content: 'Recovered ' });
    send({ content: 'answer' });
    expect(draft()).toBe('Recovered answer');
    await act(async () => { resolveChat?.({ ok: false, error: 'fixture finished' }); await sending; });
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
  it('refreshes the sidebar bookmark query when a message is bookmarked', async () => {
    let bookmarks: Array<{ id: string; spaceId: string }> = [];
    api.listMessageBookmarks = vi.fn().mockImplementation(async () => ({ ok: true, data: { bookmarks, total: bookmarks.length } }));
    api.bookmarkConversationMessage = vi.fn().mockImplementation(async () => {
      bookmarks = [{ id: 'bookmark-1', spaceId: 'space_general' }];
      return { ok: true, data: { status: 'success' } };
    });
    const { result } = renderHook(() => ({ controller: useConversationsStore(), sidebar: useSidebarBookmarksQuery('', null) }), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.sidebar.isSuccess).toBe(true));
    expect(result.current.sidebar.data).toEqual([]);
    await act(async () => { await result.current.controller.bookmarkMessage('conversation-1', 'message-1'); });
    await waitFor(() => expect(result.current.sidebar.data).toEqual(bookmarks));
  });

});
