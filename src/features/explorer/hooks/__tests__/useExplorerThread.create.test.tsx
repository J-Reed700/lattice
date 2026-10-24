import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

import { useExplorerThread } from '../useExplorerThread';

const mocks = vi.hoisted(() => ({
  api: {
    listConversationsExplorer: vi.fn(),
    listConversations: vi.fn(),
    getConversation: vi.fn(),
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

const ROOT = '/Users/me/herbarium';
const thread = (id: string, updatedAt: string, explorerRoot = ROOT) => ({ id, title: id, updatedAt, explorerRoot });
const folders = (lastThreadId: string | null = null) => ({ ok: true, data: { home: null, folders: [{ root: ROOT, name: 'herbarium', lastThreadId }] } });
const list = (...conversations: unknown[]) => ({ ok: true, data: { conversations } });

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date(2026, 9, 24, 9, 5));
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  mocks.store.activeConversationId = null;
  mocks.api.explorerFoldersList.mockResolvedValue(folders());
  mocks.api.explorerFolderSetLastThread.mockResolvedValue({ ok: true, data: undefined });
  mocks.api.setConversationExplorerRoot.mockResolvedValue({ ok: true, data: undefined });
  mocks.api.getConversation.mockResolvedValue({ ok: true, data: { conversation: null } });
});
afterEach(() => vi.useRealTimers());

describe('useExplorerThread', () => {
  it('starts a dated thread in General for a folder with none, binds it and shows it at once', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValueOnce(list(thread('elsewhere', '2026-10-01T00:00:00Z', '/other')));
    mocks.api.listConversationsExplorer.mockResolvedValue(list(thread('fresh', '2026-10-24T09:05:00Z')));
    mocks.store.createConversation.mockImplementation(async (title: string) => {
      client.setQueryData(conversationKeys.detail('fresh'), { id: 'fresh', title, updatedAt: '2026-10-24T09:05:00Z', explorerRoot: null });
      return 'fresh';
    });
    const { result } = renderHook(() => useExplorerThread(ROOT, 'herbarium'), { wrapper });
    expect(result.current.preparing).toBe(true);

    await waitFor(() => expect(mocks.store.createConversation).toHaveBeenCalledTimes(1));
    const [title, space] = mocks.store.createConversation.mock.calls[0];
    expect(title).toMatch(/^herbarium · /);
    expect(space).toBe(GENERAL_SPACE_ID);
    await waitFor(() => expect(mocks.api.setConversationExplorerRoot).toHaveBeenCalledWith('fresh', ROOT));
    await waitFor(() => expect(result.current.threads.map((t) => t.id)).toEqual(['fresh']));
    expect(client.getQueryData<{ explorerRoot: string }>(conversationKeys.detail('fresh'))?.explorerRoot).toBe(ROOT);
    await waitFor(() => expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledWith(ROOT, 'fresh'));
    await waitFor(() => expect(mocks.store.selectConversation).toHaveBeenCalledWith('fresh'));
  });

  it('reports a thread it could not bind, stops preparing, and retries on request', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue(list());
    mocks.store.createConversation.mockResolvedValue('loose');
    mocks.api.setConversationExplorerRoot.mockResolvedValueOnce({ ok: false, error: 'folder is not indexed' });
    const { result } = renderHook(() => useExplorerThread(ROOT, 'herbarium'), { wrapper });
    await waitFor(() => expect(result.current.error).toBe('folder is not indexed'));
    expect(result.current.preparing).toBe(false);
    expect(mocks.store.createConversation).toHaveBeenCalledTimes(1);

    act(() => result.current.retry());
    await waitFor(() => expect(mocks.store.createConversation).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current.error).toBeNull());
  });

  it('reports a create that threw something other than an Error', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue(list());
    mocks.store.createConversation.mockRejectedValue('database locked');
    const { result } = renderHook(() => useExplorerThread(ROOT, 'herbarium'), { wrapper });
    await waitFor(() => expect(result.current.error).toBe('database locked'));
  });

  it('reports a thread list that could not be read', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue({ ok: false, error: 'unused' });
    mocks.api.listConversations.mockResolvedValue({ ok: false, error: 'conversations unavailable' });
    const { result } = renderHook(() => useExplorerThread(ROOT, 'herbarium'), { wrapper });
    await waitFor(() => expect(result.current.error).toBe('conversations unavailable'));
    expect(result.current.preparing).toBe(false);
    expect(mocks.store.createConversation).not.toHaveBeenCalled();
  });

  it('selects a thread and remembers it for the folder', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue(list(thread('a', '2026-10-01T00:00:00Z'), thread('b', '2026-10-02T00:00:00Z')));
    mocks.store.activeConversationId = 'b';
    mocks.api.explorerFoldersList.mockResolvedValue(folders('b'));
    const { result } = renderHook(() => useExplorerThread(ROOT, 'herbarium'), { wrapper });
    await waitFor(() => expect(result.current.activeId).toBe('b'));
    expect(result.current.preparing).toBe(false);
    act(() => result.current.select('a'));
    expect(mocks.store.selectConversation).toHaveBeenCalledWith('a');
    await waitFor(() => expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledWith(ROOT, 'a'));
    expect(result.current.remove).toBe(mocks.store.deleteConversation);
  });

  it('writes the remembered thread again after a failed save', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue(list(thread('a', '2026-10-01T00:00:00Z')));
    mocks.api.explorerFolderSetLastThread.mockResolvedValueOnce({ ok: false, error: 'busy' });
    const { result } = renderHook(() => useExplorerThread(ROOT, 'herbarium'), { wrapper });
    await waitFor(() => expect(result.current.threads).toHaveLength(1));
    act(() => result.current.select('a'));
    await waitFor(() => expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledTimes(1));
    act(() => result.current.select('a'));
    await waitFor(() => expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledTimes(2));
  });

  it('does nothing without a folder', async () => {
    const { result } = renderHook(() => useExplorerThread(null, ''), { wrapper });
    expect(result.current.preparing).toBe(false);
    await act(() => result.current.create());
    act(() => result.current.select('a'));
    expect(mocks.store.createConversation).not.toHaveBeenCalled();
    expect(mocks.api.listConversationsExplorer).not.toHaveBeenCalled();
    expect(mocks.api.explorerFolderSetLastThread).not.toHaveBeenCalled();
  });
});
