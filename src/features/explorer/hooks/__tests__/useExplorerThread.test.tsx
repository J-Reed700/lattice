import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';

import { useExplorerThread } from '../useExplorerThread';

const mocks = vi.hoisted(() => ({
  api: {
    listConversationsExplorer: vi.fn(),
    listConversations: vi.fn(),
    explorerFoldersList: vi.fn(),
    explorerFolderSetLastThread: vi.fn(),
    setConversationExplorerRoot: vi.fn(),
  },
  store: {
    activeConversationId: null as string | null,
    selectConversation: vi.fn(),
    createConversation: vi.fn(),
    deleteConversation: vi.fn(),
  },
}));
vi.mock('@/lib/api', () => ({ VaultAPI: mocks.api, default: mocks.api }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: typeof mocks.store) => unknown) => select(mocks.store),
}));

const ROOT = '/Users/me/project';
const thread = (id: string, updatedAt: string) => ({ id, title: id, updatedAt, explorerRoot: ROOT });
const folder = (lastThreadId: string | null) => ({
  root: ROOT, name: 'project', pinned: false, addedAt: '2026-10-01T10:00:00.000Z', lastOpenedAt: '2026-10-09T10:00:00.000Z',
  exists: true, threadCount: 2, instructions: null, spaceId: 'space_general', lastThreadId,
  index: { state: 'indexed', filesTotal: 1, filesIndexed: 1, passagesTotal: 1, passagesEmbedded: 1, bytes: 1, indexRoot: null, etaSeconds: null, message: null },
});

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

beforeEach(() => {
  vi.clearAllMocks();
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  conversationUiStore.setState({ activeConversationId: 'chat-main-1' });
  mocks.store.activeConversationId = 'chat-main-1';
  mocks.api.listConversationsExplorer.mockResolvedValue({
    ok: true,
    data: { conversations: [thread('older', '2026-10-01T10:00:00Z'), thread('newer', '2026-10-08T10:00:00Z')] },
  });
  mocks.api.explorerFolderSetLastThread.mockResolvedValue({ ok: true, data: null });
});

describe('the thread a folder opens on', () => {
  it('is the one its row remembers, not the newest, without touching /chat activeConversationId', async () => {
    mocks.api.explorerFoldersList.mockResolvedValue({ ok: true, data: { home: null, folders: [folder('older')] } });
    const { result } = renderHook(() => useExplorerThread(ROOT, 'project'), { wrapper });
    await waitFor(() => expect(result.current.activeId).toBe('older'));
    expect(mocks.store.selectConversation).not.toHaveBeenCalled();
    expect(conversationUiStore.getState().activeConversationId).toBe('chat-main-1');
  });

  it('is the newest when the remembered one is gone', async () => {
    mocks.api.explorerFoldersList.mockResolvedValue({ ok: true, data: { home: null, folders: [folder('deleted')] } });
    const { result } = renderHook(() => useExplorerThread(ROOT, 'project'), { wrapper });
    await waitFor(() => expect(result.current.activeId).toBe('newer'));
    expect(mocks.store.selectConversation).not.toHaveBeenCalled();
    expect(conversationUiStore.getState().activeConversationId).toBe('chat-main-1');
  });

  it('is saved on the folder\'s row when another thread is shown, once', async () => {
    mocks.api.explorerFoldersList.mockResolvedValue({ ok: true, data: { home: null, folders: [folder('older')] } });
    const { result, rerender } = renderHook(() => useExplorerThread(ROOT, 'project'), { wrapper });
    await waitFor(() => expect(result.current.activeId).toBe('older'));
    expect(mocks.api.explorerFolderSetLastThread).not.toHaveBeenCalled();

    act(() => result.current.select('newer'));
    rerender();
    await waitFor(() => expect(result.current.activeId).toBe('newer'));
    await waitFor(() => expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledWith(ROOT, 'newer'));
    act(() => result.current.select('newer'));
    expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledOnce();
    expect(conversationUiStore.getState().activeConversationId).toBe('chat-main-1');
  });
});
