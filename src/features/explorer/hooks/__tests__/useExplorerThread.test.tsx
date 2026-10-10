import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

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
  mocks.store.activeConversationId = null;
  mocks.api.listConversationsExplorer.mockResolvedValue({
    ok: true,
    data: { conversations: [thread('older', '2026-10-01T10:00:00Z'), thread('newer', '2026-10-08T10:00:00Z')] },
  });
  mocks.api.explorerFolderSetLastThread.mockResolvedValue({ ok: true, data: undefined });
});

describe('the thread a folder opens on', () => {
  it('is the one its row remembers, not the newest', async () => {
    mocks.api.explorerFoldersList.mockResolvedValue({ ok: true, data: { home: null, folders: [folder('older')] } });
    renderHook(() => useExplorerThread(ROOT, 'project'), { wrapper });
    await waitFor(() => expect(mocks.store.selectConversation).toHaveBeenCalledWith('older'));
    expect(mocks.store.selectConversation).not.toHaveBeenCalledWith('newer');
  });

  it('is the newest when the remembered one is gone', async () => {
    mocks.api.explorerFoldersList.mockResolvedValue({ ok: true, data: { home: null, folders: [folder('deleted')] } });
    renderHook(() => useExplorerThread(ROOT, 'project'), { wrapper });
    await waitFor(() => expect(mocks.store.selectConversation).toHaveBeenCalledWith('newer'));
  });

  it('is saved on the folder\'s row when another thread is shown, once', async () => {
    mocks.api.explorerFoldersList.mockResolvedValue({ ok: true, data: { home: null, folders: [folder('older')] } });
    const { rerender } = renderHook(() => useExplorerThread(ROOT, 'project'), { wrapper });
    await waitFor(() => expect(mocks.store.selectConversation).toHaveBeenCalledWith('older'));

    mocks.store.activeConversationId = 'older';
    rerender();
    expect(mocks.api.explorerFolderSetLastThread).not.toHaveBeenCalled();

    mocks.store.activeConversationId = 'newer';
    rerender();
    rerender();
    await waitFor(() => expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledWith(ROOT, 'newer'));
    await act(async () => { rerender(); });
    expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledOnce();
  });
});
