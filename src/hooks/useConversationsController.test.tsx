import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useSidebarBookmarksQuery } from '@/components/Chat/sidebar/workspaceQueries';
import { conversationKeys } from '@/hooks/queries/conversationKeys';
import { MAX_OBSERVED_CONVERSATIONS, MAX_OBSERVED_MEMBERSHIPS } from '@/hooks/useConversationsController';
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

const createWrapper = (queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })) => function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <ConversationsProvider>{children}</ConversationsProvider>
      </QueryClientProvider>
    );
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

  it('keeps unrelated selector consumers asleep during streaming state changes', async () => {
    let renders = 0;
    const { result } = renderHook(() => {
      renders += 1;
      return useConversationsStore((state) => state.selectedSpaceId);
    }, { wrapper: createWrapper() });
    await waitFor(() => expect(result.current).toBeNull());
    const before = renders;

    act(() => conversationUiStore.setState({ optimisticMessages: new Map() }));

    await waitFor(() => expect(renders).toBe(before));
  });

  it('retains captured web text when loading citation metadata', async () => {
    const webSnapshot = {
      url: 'https://example.com/growing',
      title: 'Growing guide',
      text: 'The original article, preserved even after the website changes.',
      fetchedAt: '2026-09-24T00:00:00.000Z',
      truncated: false,
    };
    conversationUiStore.setState({ activeConversationId: conversation.id });
    api.getConversationMessages.mockResolvedValue({
      ok: true,
      data: { messages: [{
        id: 'saved-answer', conversationId: conversation.id, role: 'assistant',
        content: 'A claim [1].', tokens: 5, status: 'completed',
        createdAt: '2026-09-24T00:00:00.000Z',
        metadata: JSON.stringify({ sources: [{
          documentId: 'web:https://example.com/growing', chunkId: 'web-chunk',
          fileName: 'Growing guide', filePath: webSnapshot.url,
          mimeType: 'text/html', category: 'Web Article', content: 'Saved search excerpt',
          score: 1, fileSizeBytes: 0, modifiedAt: '', citationId: 1, webSnapshot,
        }] }),
      }], total: 1 },
    });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.lastMessageSources.get('saved-answer')?.[0]?.webSnapshot)
      .toEqual(webSnapshot));
    expect(result.current.lastMessageSources.get('saved-answer')?.[0]?.content).toBe('Saved search excerpt');
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

  it('does not reconcile a failed prompt against an older persisted question with the same text', async () => {
    api.chatWithConversation = vi.fn().mockResolvedValue({ ok: false, error: 'provider failed' });
    api.getConversationMessages = vi.fn().mockResolvedValue({
      ok: true,
      data: { messages: [{
        id: 'old-question', conversationId: 'conversation-1', role: 'user', content: 'Same question',
        tokens: 2, status: 'completed', createdAt: '2026-09-01T00:00:00.000Z',
        metadata: JSON.stringify({ requestId: 'older-request' }),
      }], total: 1 },
    });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });

    await act(async () => { await result.current.sendMessage('Same question', 'conversation-1'); });

    expect([...result.current.optimisticMessages.values()]).toEqual([
      expect.objectContaining({ content: 'Same question', status: 'failed' }),
    ]);
  });

  it('reconciles a failure only when its exact request ID was persisted', async () => {
    let persistedRequestId = '';
    api.chatWithConversation = vi.fn().mockImplementation(async (...args: unknown[]) => {
      persistedRequestId = args[3] as string;
      return { ok: false, error: 'provider failed after saving the question' };
    });
    api.getConversationMessages = vi.fn().mockImplementation(async () => ({
      ok: true,
      data: { messages: [{
        id: 'saved-question', conversationId: 'conversation-1', role: 'user', content: 'Same question',
        tokens: 2, status: 'completed', createdAt: '2026-09-30T00:00:00.000Z',
        metadata: JSON.stringify({ requestId: persistedRequestId }),
      }], total: 1 },
    }));
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });

    await act(async () => { await result.current.sendMessage('Same question', 'conversation-1'); });

    expect(persistedRequestId).not.toBe('');
    expect(result.current.optimisticMessages.size).toBe(0);
  });

  /**
   * A retry used to arrive as text and overwrite the answer bubble, so a turn
   * that recovered showed "Model response failed. Retrying..." where its answer
   * should have been. It is a step now: the timeline records it and the answer
   * keeps accumulating.
   */
  it('records a retry as a step without touching the answer', async () => {
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
      conversationId: 'conversation-1', requestId, done: false, ...payload,
    } }));
    const draft = () => [...result.current.optimisticMessages.values()].find(message => message.role === 'assistant')?.content;
    const steps = () => result.current.liveSteps.get('conversation-1') ?? [];

    send({ content: 'Partial answer. ' });
    await waitFor(() => expect(draft()).toBe('Partial answer. '));

    const retryStep = {
      id: 's3', kind: 'retry', label: 'Asking again — the model returned nothing',
      state: 'done', startedAtMs: 400, durationMs: 0, result: 'attempt 2',
    };
    // Another generation on the same channel must not reach this turn.
    send({ status: 'step', step: retryStep, requestId: 'unrelated' });
    expect(steps()).toHaveLength(0);

    send({ status: 'step', step: retryStep });
    expect(draft()).toBe('Partial answer. ');
    expect(steps()).toEqual([expect.objectContaining({ id: 's3', kind: 'retry' })]);

    send({ content: 'Recovered ' });
    send({ content: 'answer' });
    await waitFor(() => expect(draft()).toBe('Partial answer. Recovered answer'));
    await act(async () => { resolveChat?.({ ok: false, error: 'fixture finished' }); await sending; });
  });

  /**
   * The answer returns before its grounding check; the check lands on the
   * same channel afterwards and must reach the badge without a reload.
   */
  it('fills in the answer\'s check when it lands after the turn returned', async () => {
    let emit: ((event: { payload: Record<string, unknown> }) => void) | undefined;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementationOnce(async (_event, handler) => {
      emit = handler as typeof emit;
      return unlisten;
    });
    conversationUiStore.setState({ activeConversationId: 'conversation-1' });
    const answer = {
      id: 'answer-1',
      conversationId: 'conversation-1',
      role: 'assistant',
      content: 'Tomatoes need sun [1].',
      tokens: 5,
      status: 'completed',
      createdAt: '2026-09-24T00:00:00.000Z',
      metadata: JSON.stringify({ verification: { enabled: true, pending: true } }),
    };
    api.getConversationMessages = vi.fn().mockResolvedValue({ ok: true, data: { messages: [answer], total: 1 } });
    api.chatWithConversation = vi.fn().mockResolvedValue({
      ok: true,
      data: { conversationId: 'conversation-1', message: answer.content, messages: [answer], contextUsed: 0, sources: [] },
    });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));

    await act(async () => { await result.current.sendMessage('Where do tomatoes go?'); });
    expect(result.current.messageVerification.get('answer-1')?.pending).toBe(true);
    // Still listening: the check has not arrived.
    expect(unlisten).not.toHaveBeenCalled();

    const requestId = api.chatWithConversation.mock.calls[0][3];
    act(() => emit?.({ payload: {
      conversationId: 'conversation-1',
      requestId,
      done: false,
      status: 'verification',
      verification: {
        messageId: 'answer-1',
        verification: {
          enabled: true,
          claimsEvaluated: 1,
          supportedClaims: 1,
          verdictCounts: { supported: 1, contradicted: 0, unsupported: 0, unverified: 0 },
        },
      },
    } }));

    await waitFor(() => expect(result.current.messageVerification.get('answer-1')?.claimsEvaluated).toBe(1));
    expect(result.current.messageVerification.get('answer-1')?.pending).toBeUndefined();
    expect(unlisten).toHaveBeenCalled();
  });

  /**
   * A finish event carries the id of its start. Appending it would make a
   * finished step jump to the bottom of the timeline the moment it completed.
   */
  it('merges a step finish into the step it started, in place', async () => {
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
    act(() => { sending = result.current.sendMessage('Search my notes'); });
    await waitFor(() => expect(api.chatWithConversation).toHaveBeenCalled());
    const requestId = api.chatWithConversation.mock.calls[0][3];
    const send = (payload: Record<string, unknown>) => act(() => emit?.({ payload: {
      conversationId: 'conversation-1', requestId, done: false, ...payload,
    } }));

    send({ status: 'step', step: { id: 's0', kind: 'plan', label: 'Planning what to search for', state: 'running', startedAtMs: 0 } });
    send({ status: 'step', step: { id: 's1', kind: 'search_documents', label: 'Searching your documents', state: 'running', startedAtMs: 800 } });
    send({ status: 'step', step: { id: 's0', kind: 'plan', label: 'Planning what to search for', state: 'done', startedAtMs: 0, durationMs: 780 } });

    const steps = result.current.liveSteps.get('conversation-1') ?? [];
    expect(steps.map(step => step.id)).toEqual(['s0', 's1']);
    expect(steps[0]).toMatchObject({ state: 'done', durationMs: 780 });

    // Cleared exactly where the live retrieval trace is cleared.
    await act(async () => { resolveChat?.({ ok: false, error: 'fixture finished' }); await sending; });
    expect(result.current.liveSteps.has('conversation-1')).toBe(false);
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

  const cancelMidTurn = async (persistedMessages: unknown[]) => {
    conversationUiStore.setState({ composerDraft: null });
    let resolveChat: ((value: unknown) => void) | undefined;
    api.chatWithConversation = vi.fn().mockImplementation(() => new Promise(resolve => {
      resolveChat = resolve;
    }));
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    let sendPromise: Promise<void> | undefined;
    act(() => { sendPromise = result.current.sendMessage('Where is the draft?'); });
    await waitFor(() => expect(result.current.inFlightGenerations.size).toBe(1));
    api.getConversationMessages = vi.fn().mockResolvedValue({
      ok: true,
      data: { messages: persistedMessages, total: persistedMessages.length },
    });
    await act(async () => result.current.cancelGeneration('conversation-1'));
    await act(async () => {
      resolveChat?.({ ok: false, error: 'generation cancelled', details: { code: ErrorCode.INVALID_STATE } });
      await sendPromise;
    });
    return result;
  };

  it('hands the question back to the composer when a stop lands before it was saved', async () => {
    const result = await cancelMidTurn([]);
    expect(result.current.composerDraft).toBe('Where is the draft?');
  });

  it('leaves the composer alone when the stopped question was already saved', async () => {
    const result = await cancelMidTurn([{
      id: 'm1',
      conversationId: 'conversation-1',
      role: 'user',
      content: 'Where is the draft?',
      status: 'failed',
      createdAt: '2026-08-02T00:00:00.000Z',
    }]);
    expect(result.current.composerDraft).toBeNull();
  });

  it('forwards the attached document ids to the chat call', async () => {
    api.chatWithConversation = vi.fn().mockResolvedValue({ ok: false, error: 'fixture finished' });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    await act(async () => result.current.sendMessage('Read these', 'conversation-1', undefined, ['a.pdf'], ['doc-1']));
    expect(api.chatWithConversation).toHaveBeenCalledWith(
      'conversation-1', 'Read these', undefined, expect.any(String), ['a.pdf'], ['doc-1']
    );
  });

  it('retains attachment names, document IDs and tool preferences on a failed-message retry', async () => {
    api.chatWithConversation = vi.fn().mockResolvedValue({ ok: false, error: 'fixture failure' });
    api.getConversationMessages = vi.fn().mockResolvedValue({ ok: true, data: { messages: [], total: 0 } });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    const preferences = { knowledgeBase: false, webSearch: true, turnMode: 'query' as const, enabledTools: ['web'] };
    await act(async () => {
      await result.current.sendMessage('Use the selected manual', 'conversation-1', preferences, ['Manual.pdf'], ['doc-manual']);
    });
    const failed = [...result.current.optimisticMessages.values()].find(message => message.role === 'user');
    expect(failed?.retryContext).toEqual({
      toolPreferences: preferences,
      attachmentNames: ['Manual.pdf'],
      attachmentDocumentIds: ['doc-manual'],
    });

    api.chatWithConversation.mockClear();
    await act(async () => { await result.current.retryFailedMessage(failed!.tempId); });

    expect(api.chatWithConversation).toHaveBeenCalledWith(
      'conversation-1', 'Use the selected manual', preferences, expect.any(String),
      ['Manual.pdf'], ['doc-manual']
    );
  });

  it('dismisses only the selected failed user prompt', async () => {
    api.chatWithConversation = vi.fn().mockResolvedValue({ ok: false, error: 'fixture failure' });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await act(async () => { await result.current.sendMessage('Keep until dismissed', 'conversation-1'); });
    const failed = [...result.current.optimisticMessages.values()].find(message => message.role === 'user');
    expect(failed).toBeDefined();

    act(() => result.current.dismissFailedMessage(failed!.tempId));

    expect(result.current.optimisticMessages.size).toBe(0);
  });

  it('deleting a conversation releases a pending turn and prevents its late response repopulating caches', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    conversationUiStore.setState({ activeConversationId: conversation.id });
    let deleted = false;
    api.listConversationsExplorer = vi.fn().mockImplementation(async () => ({
      ok: true,
      data: { conversations: deleted ? [] : [conversation], total: deleted ? 0 : 1 },
    }));
    let resolveChat: ((value: unknown) => void) | undefined;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementationOnce(async () => unlisten);
    api.chatWithConversation = vi.fn().mockImplementation(() => new Promise(resolve => { resolveChat = resolve; }));
    api.deleteConversation = vi.fn().mockImplementation(async () => {
      deleted = true;
      return { ok: true, data: undefined };
    });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper(queryClient) });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    let sending!: Promise<void>;
    act(() => { sending = result.current.sendMessage('Must not return after deletion', conversation.id); });
    await waitFor(() => expect(api.chatWithConversation).toHaveBeenCalled());
    await waitFor(() => expect(listen).toHaveBeenCalled());

    await act(async () => { await result.current.deleteConversation(conversation.id); });
    expect(unlisten).toHaveBeenCalled();
    expect(result.current.inFlightGenerations.has(conversation.id)).toBe(false);
    expect(result.current.optimisticMessages.size).toBe(0);

    await act(async () => {
      resolveChat?.({
        ok: true,
        data: { conversationId: conversation.id, message: 'late answer', messages: [], contextUsed: 0, sources: [] },
      });
      await sending;
    });

    expect(queryClient.getQueryData(conversationKeys.messages(conversation.id))).toBeUndefined();
    expect(result.current.conversations.some(item => item.id === conversation.id)).toBe(false);
  });

  it('keeps a deleted conversation out of lists when an older list read was pending', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    let deleted = false;
    let resolveOldList: ((value: unknown) => void) | undefined;
    let listCalls = 0;
    api.listConversationsExplorer = vi.fn().mockImplementation(() => {
      listCalls += 1;
      if (listCalls === 2) {
        return new Promise(resolve => { resolveOldList = resolve; });
      }
      return Promise.resolve({
        ok: true,
        data: { conversations: deleted ? [] : [conversation], total: deleted ? 0 : 1 },
      });
    });
    api.deleteConversation = vi.fn().mockImplementation(async () => {
      deleted = true;
      return { ok: true, data: undefined };
    });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper(queryClient) });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));

    const oldRead = queryClient.refetchQueries({ queryKey: conversationKeys.lists, type: 'active' });
    await waitFor(() => expect(listCalls).toBe(2));
    await act(async () => { await result.current.deleteConversation(conversation.id); });
    resolveOldList?.({ ok: true, data: { conversations: [conversation], total: 1 } });
    await oldRead;

    await waitFor(() => expect(result.current.conversations).toEqual([]));
    expect(queryClient.getQueryData(conversationKeys.list({ spaceId: null, filterMode: 'all', searchQuery: '' })))
      .toEqual([]);
  });

  it('restores only its own list row when a failed deletion overlaps another successful deletion', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const other = { id: 'conversation-2', title: 'Other chat', updatedAt: conversation.updatedAt };
    const deleted = new Set<string>();
    let resolveFirstDelete: ((value: unknown) => void) | undefined;
    api.listConversationsExplorer = vi.fn().mockImplementation(async () => {
      const conversations = [conversation, other].filter(item => !deleted.has(item.id));
      return { ok: true, data: { conversations, total: conversations.length } };
    });
    api.deleteConversation = vi.fn().mockImplementation((id: string) => {
      if (id === conversation.id) return new Promise(resolve => { resolveFirstDelete = resolve; });
      deleted.add(id);
      return Promise.resolve({ ok: true, data: undefined });
    });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper(queryClient) });
    await waitFor(() => expect(result.current.conversations).toHaveLength(2));

    let deletingFirst!: Promise<void>;
    act(() => { deletingFirst = result.current.deleteConversation(conversation.id); });
    await waitFor(() => expect(api.deleteConversation).toHaveBeenCalledWith(conversation.id));
    await act(async () => { await result.current.deleteConversation(other.id); });
    await act(async () => {
      resolveFirstDelete?.({ ok: false, error: 'first delete failed' });
      await deletingFirst;
    });

    expect(result.current.conversations.map(item => item.id)).toEqual([conversation.id]);
  });

  it('does not let a pending truncate recreate messages after the conversation is deleted', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const original = {
      id: 'message-before-truncate', conversationId: conversation.id, role: 'user',
      content: 'Keep this history', tokens: 3, status: 'completed', createdAt: '2026-09-30T00:00:00.000Z',
    } as const;
    queryClient.setQueryData(conversationKeys.messages(conversation.id), [original]);
    let resolveTruncate: ((value: unknown) => void) | undefined;
    api.truncateConversationAfter = vi.fn().mockImplementation(() => new Promise(resolve => { resolveTruncate = resolve; }));
    api.deleteConversation = vi.fn().mockResolvedValue({ ok: true, data: undefined });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper(queryClient) });

    let truncating!: Promise<boolean>;
    act(() => { truncating = result.current.truncateAfter(conversation.id, original.id); });
    await waitFor(() => expect(api.truncateConversationAfter).toHaveBeenCalled());
    await act(async () => { await result.current.deleteConversation(conversation.id); });
    expect(queryClient.getQueryData(conversationKeys.messages(conversation.id))).toBeUndefined();

    await act(async () => {
      resolveTruncate?.({ ok: true, data: { messages: [original] } });
      await expect(truncating).resolves.toBe(false);
    });
    expect(queryClient.getQueryData(conversationKeys.messages(conversation.id))).toBeUndefined();
  });

  it('does not restore a failed message deletion after its conversation is deleted', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const savedMessage = {
      id: 'message-being-deleted', conversationId: conversation.id, role: 'user',
      content: 'This conversation will be removed', tokens: 5, status: 'completed',
      createdAt: '2026-09-30T00:00:00.000Z',
    } as const;
    queryClient.setQueryData(conversationKeys.messages(conversation.id), [savedMessage]);
    let resolveMessageDelete: ((value: unknown) => void) | undefined;
    api.deleteConversationMessage = vi.fn().mockImplementation(() => new Promise(resolve => { resolveMessageDelete = resolve; }));
    api.deleteConversation = vi.fn().mockResolvedValue({ ok: true, data: undefined });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper(queryClient) });

    let deletingMessage!: Promise<void>;
    act(() => { deletingMessage = result.current.deleteMessage(conversation.id, savedMessage.id); });
    await waitFor(() => expect(api.deleteConversationMessage).toHaveBeenCalled());
    await act(async () => { await result.current.deleteConversation(conversation.id); });
    expect(queryClient.getQueryData(conversationKeys.messages(conversation.id))).toBeUndefined();

    await act(async () => {
      resolveMessageDelete?.({ ok: false, error: 'message deletion failed too late' });
      await deletingMessage;
    });
    expect(queryClient.getQueryData(conversationKeys.messages(conversation.id))).toBeUndefined();
    expect(result.current.error).toBeNull();
  });

  it('restores only the failed message while preserving messages added during deletion', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const precedingMessage = {
      id: 'preceding-message', conversationId: conversation.id, role: 'user',
      content: 'An earlier message', tokens: 3, status: 'completed', createdAt: '2026-09-30T00:00:00.000Z',
    } as const;
    const savedMessage = {
      id: 'message-being-deleted', conversationId: conversation.id, role: 'user',
      content: 'Keep this message', tokens: 4, status: 'completed', createdAt: '2026-09-30T00:00:01.000Z',
    } as const;
    const followingMessage = {
      id: 'following-message', conversationId: conversation.id, role: 'assistant',
      content: 'A later saved answer', tokens: 4, status: 'completed', createdAt: '2026-09-30T00:00:02.000Z',
    } as const;
    const nextMessage = {
      id: 'message-arrived-later', conversationId: conversation.id, role: 'assistant',
      content: 'Concurrent response', tokens: 3, status: 'completed', createdAt: '2026-09-30T00:00:03.000Z',
    } as const;
    queryClient.setQueryData(conversationKeys.messages(conversation.id), [precedingMessage, savedMessage, followingMessage]);
    let resolveMessageDelete: ((value: unknown) => void) | undefined;
    api.deleteConversationMessage = vi.fn().mockImplementation(() => new Promise(resolve => { resolveMessageDelete = resolve; }));
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper(queryClient) });

    let deletingMessage!: Promise<void>;
    act(() => { deletingMessage = result.current.deleteMessage(conversation.id, savedMessage.id); });
    await waitFor(() => expect(api.deleteConversationMessage).toHaveBeenCalled());
    // The preceding message was concurrently removed, while later messages
    // and a newly appended answer should keep their order around the rollback.
    queryClient.setQueryData(conversationKeys.messages(conversation.id), [followingMessage, nextMessage]);
    await act(async () => {
      resolveMessageDelete?.({ ok: false, error: 'message deletion failed' });
      await deletingMessage;
    });

    expect(queryClient.getQueryData(conversationKeys.messages(conversation.id))).toEqual([savedMessage, followingMessage, nextMessage]);
  });

  it('does not surface a failed read from a conversation opened before the current one', async () => {
    let rejectOlderRead: ((value: unknown) => void) | undefined;
    api.getConversationMessages = vi.fn()
      .mockImplementationOnce(() => new Promise(resolve => { rejectOlderRead = resolve; }))
      .mockResolvedValue({ ok: true, data: { messages: [], total: 0 } });
    api.getConversation = vi.fn().mockResolvedValue({ ok: true, data: { conversation } });
    api.listMessageBookmarks = vi.fn().mockResolvedValue({ ok: true, data: { bookmarks: [], total: 0 } });
    api.listConversationLinkedDocuments = vi.fn().mockResolvedValue({ ok: true, data: [] });
    api.listConversationWebSources = vi.fn().mockResolvedValue({ ok: true, data: [] });
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });

    let openingOlder!: Promise<void>;
    act(() => { openingOlder = result.current.selectConversation('older-chat'); });
    await waitFor(() => expect(api.getConversationMessages).toHaveBeenCalledTimes(1));
    await act(async () => { await result.current.selectConversation('current-chat'); });
    await act(async () => {
      rejectOlderRead?.({ ok: false, error: 'older chat is unavailable' });
      await openingOlder;
    });

    expect(result.current.activeConversationId).toBe('current-chat');
    expect(result.current.error).toBeNull();
  });

  it('keeps a pending turn intact when the backend refuses conversation deletion', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    conversationUiStore.setState({ activeConversationId: conversation.id });
    let resolveChat: ((value: unknown) => void) | undefined;
    let resolveDelete: ((value: unknown) => void) | undefined;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementationOnce(async () => unlisten);
    api.chatWithConversation = vi.fn().mockImplementation(() => new Promise(resolve => { resolveChat = resolve; }));
    api.deleteConversation = vi.fn().mockImplementation(() => new Promise(resolve => { resolveDelete = resolve; }));
    const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper(queryClient) });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    let sending!: Promise<void>;
    act(() => { sending = result.current.sendMessage('Keep this turn', conversation.id); });
    await waitFor(() => expect(api.chatWithConversation).toHaveBeenCalled());

    let deleting!: Promise<void>;
    act(() => { deleting = result.current.deleteConversation(conversation.id); });
    await waitFor(() => expect(api.deleteConversation).toHaveBeenCalled());
    expect(result.current.activeConversationId).toBeNull();
    expect(result.current.inFlightGenerations.has(conversation.id)).toBe(true);
    expect([...result.current.optimisticMessages.values()]).toEqual([
      expect.objectContaining({ content: 'Keep this turn', status: 'pending' }),
      expect.objectContaining({ role: 'assistant', status: 'pending' }),
    ]);
    expect(unlisten).not.toHaveBeenCalled();

    const answer = {
      id: 'answer-after-delete-race', conversationId: conversation.id, role: 'assistant',
      content: 'The answer survived.', tokens: 4, status: 'completed', createdAt: '2026-09-30T00:00:00.000Z',
    };
    await act(async () => {
      resolveChat?.({ ok: true, data: {
        conversationId: conversation.id, message: answer.content, messages: [answer], contextUsed: 0, sources: [],
      } });
      await sending;
    });
    await act(async () => {
      resolveDelete?.({ ok: false, error: 'delete failed' });
      await deleting;
    });
    expect(result.current.activeConversationId).toBe(conversation.id);
    expect(queryClient.getQueryData(conversationKeys.messages(conversation.id))).toEqual([answer]);
  });

  it('settles an unmounted send as a retryable prompt when its request returns', async () => {
    let resolveChat: ((value: unknown) => void) | undefined;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementationOnce(async () => unlisten);
    api.chatWithConversation = vi.fn().mockImplementation(() => new Promise(resolve => { resolveChat = resolve; }));
    const { result, unmount } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    const preferences = { knowledgeBase: false, webSearch: true, turnMode: 'query' as const };
    let sending!: Promise<void>;
    act(() => {
      sending = result.current.sendMessage('Keep this unsaved prompt', conversation.id, preferences, ['Guide.pdf'], ['doc-guide']);
    });
    await waitFor(() => expect(api.chatWithConversation).toHaveBeenCalled());

    unmount();
    expect(unlisten).toHaveBeenCalled();
    await act(async () => {
      resolveChat?.({ ok: false, error: 'request finished after provider unmount' });
      await sending;
    });

    const ui = conversationUiStore.getState();
    expect(ui.inFlightGenerations.has(conversation.id)).toBe(false);
    expect([...ui.optimisticMessages.values()]).toEqual([
      expect.objectContaining({
        role: 'user', content: 'Keep this unsaved prompt', status: 'failed',
        retryContext: {
          toolPreferences: preferences,
          attachmentNames: ['Guide.pdf'],
          attachmentDocumentIds: ['doc-guide'],
        },
      }),
    ]);
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

  describe('opening a conversation', () => {
    beforeEach(() => {
      api.getConversation = vi.fn().mockResolvedValue({ ok: true, data: { conversation: { ...conversation, spaceId: 'food-research' } } });
      api.listMessageBookmarks = vi.fn().mockResolvedValue({ ok: true, data: { bookmarks: [], total: 0 } });
      api.listConversationLinkedDocuments = vi.fn().mockResolvedValue({ ok: true, data: [] });
      api.listConversationWebSources = vi.fn().mockResolvedValue({ ok: true, data: [] });
    });

    it('moves the sidebar to the space the conversation lives in', async () => {
      // "All spaces" is selected; the chat opened belongs to Food Research.
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      await act(async () => { await result.current.selectConversation('conversation-1'); });

      expect(conversationUiStore.getState().selectedSpaceId).toBe('food-research');
      expect(api.listConversationsExplorer).toHaveBeenLastCalledWith(expect.objectContaining({ spaceId: 'food-research' }));
    });

    it('leaves the space alone when the conversation is already in it', async () => {
      conversationUiStore.setState({ selectedSpaceId: 'food-research' });
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      await waitFor(() => expect(api.listConversationsExplorer).toHaveBeenCalled());
      const before = api.listConversationsExplorer.mock.calls.length;
      await act(async () => { await result.current.selectConversation('conversation-1'); });

      expect(conversationUiStore.getState().selectedSpaceId).toBe('food-research');
      expect(api.listConversationsExplorer.mock.calls.length).toBe(before);
    });

    it('keeps the chat open when opening it moves the sidebar to its space', async () => {
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      await act(async () => { await result.current.selectConversation('conversation-1'); });

      expect(conversationUiStore.getState().activeConversationId).toBe('conversation-1');
    });

    it('closes the open chat when the reader switches to another space', async () => {
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      await act(async () => { await result.current.selectConversation('conversation-1'); });
      await act(async () => { await result.current.loadConversations({ spaceId: 'movies' }); });

      expect(conversationUiStore.getState().activeConversationId).toBeNull();
    });

    it('keeps the open chat when the reader switches to All spaces', async () => {
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      await act(async () => { await result.current.selectConversation('conversation-1'); });
      await act(async () => { await result.current.loadConversations({ spaceId: null }); });

      expect(conversationUiStore.getState().activeConversationId).toBe('conversation-1');
    });

    it('does not drag the sidebar to a chat the reader has already left', async () => {
      // The open chat's own detail query asks too, so every caller is answered.
      const waiting: ((_value: unknown) => void)[] = [];
      const resolveDetail = (value: unknown) => waiting.splice(0).forEach(resolve => resolve(value));
      api.getConversation = vi.fn().mockImplementation(() => new Promise(resolve => { waiting.push(resolve); }));
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      let opening!: Promise<void>;
      act(() => { opening = result.current.selectConversation('conversation-1'); });
      await waitFor(() => expect(api.getConversation).toHaveBeenCalled());
      act(() => { conversationUiStore.setState({ activeConversationId: 'conversation-2' }); });
      await act(async () => {
        resolveDetail({ ok: true, data: { conversation: { ...conversation, spaceId: 'food-research' } } });
        await opening;
      });

      expect(conversationUiStore.getState().selectedSpaceId).toBeNull();
    });

    it('stops observing a deleted conversation and deletes one the list filter hides', async () => {
      const hidden = 'hidden-by-search';
      api.deleteConversation = vi.fn().mockResolvedValue({ ok: true, data: undefined });
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      await act(async () => { await result.current.selectConversation(hidden); });
      expect(conversationUiStore.getState().requestedLinkedConversationIds.has(hidden)).toBe(true);
      const linkedCalls = api.listConversationLinkedDocuments.mock.calls.length;

      await act(async () => { await result.current.deleteConversation(hidden); });

      expect(api.deleteConversation).toHaveBeenCalledWith(hidden);
      expect(result.current.error).toBeNull();
      const ui = conversationUiStore.getState();
      expect(ui.requestedLinkedConversationIds.has(hidden)).toBe(false);
      expect(ui.requestedWebSourceConversationIds.has(hidden)).toBe(false);
      // No observer is left to re-create and refetch the deleted conversation.
      expect(api.listConversationLinkedDocuments.mock.calls.length).toBe(linkedCalls);
    });

    it('keeps observers only for the most recent conversations', async () => {
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      for (let i = 0; i <= MAX_OBSERVED_CONVERSATIONS; i += 1) {
        await act(async () => { await result.current.selectConversation(`c-${i}`); });
      }
      const linked = conversationUiStore.getState().requestedLinkedConversationIds;
      expect(linked.size).toBe(MAX_OBSERVED_CONVERSATIONS);
      expect(linked.has('c-0')).toBe(false);
      expect(linked.has(`c-${MAX_OBSERVED_CONVERSATIONS}`)).toBe(true);
    });

    it('bounds membership observers and reloads an evicted document on demand', async () => {
      api.listDocumentSpaceMemberships = vi.fn().mockResolvedValue({ ok: true, data: [] });
      conversationUiStore.setState({
        requestedMembershipDocumentIds: new Set(
          Array.from({ length: MAX_OBSERVED_MEMBERSHIPS }, (_, i) => `doc-${i}`)
        ),
      });
      const { result } = renderHook(() => useConversationsStore(), { wrapper: createWrapper() });
      await act(async () => { await result.current.loadDocumentSpaceMemberships('new-doc'); });
      expect(conversationUiStore.getState().requestedMembershipDocumentIds.size).toBe(MAX_OBSERVED_MEMBERSHIPS);
      expect(result.current.documentSpaceMembershipsByDocumentId.has('doc-0')).toBe(false);
      await act(async () => { await result.current.loadDocumentSpaceMemberships('doc-0'); });
      expect(result.current.documentSpaceMembershipsByDocumentId.has('doc-0')).toBe(true);
      expect(result.current.documentSpaceMembershipsByDocumentId.has('doc-1')).toBe(false);
      expect(conversationUiStore.getState().requestedMembershipDocumentIds.size).toBe(MAX_OBSERVED_MEMBERSHIPS);
    });
  });

});
