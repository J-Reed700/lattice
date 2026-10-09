import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { FolderIndexStatus } from '@/stores/explorerStore';

import {
  explorerKeys,
  INDEX_STATUS_EVENT,
  useExplorerFolderMutations,
  useExplorerIndexCommands,
  useExplorerIndexStatus,
  useExplorerIndexStatusEvents,
  useExplorerIndexStatuses,
} from './queries';

const mocks = vi.hoisted(() => ({
  api: {
    explorerIndexOpen: vi.fn(),
    explorerIndexClose: vi.fn(),
    explorerIndexRebuild: vi.fn(),
    explorerFoldersList: vi.fn(),
    explorerFolderSetLastThread: vi.fn(),
  },
  listeners: new Map<string, Set<(event: { payload: unknown }) => void>>(),
}));
vi.mock('@/lib/api', () => ({ VaultAPI: mocks.api, default: mocks.api }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    const handlers = mocks.listeners.get(name) ?? new Set();
    handlers.add(handler);
    mocks.listeners.set(name, handlers);
    return Promise.resolve(() => handlers.delete(handler));
  }),
}));

const ROOT = '/Users/me/project';
const OTHER = '/Users/me/other';

const status = (overrides: Partial<FolderIndexStatus> = {}): FolderIndexStatus => ({
  root: ROOT, indexRoot: ROOT, state: 'indexing', filesTotal: 10, filesIndexed: 4, passagesTotal: 100,
  passagesEmbedded: 40, passagesPerSecond: null, etaSeconds: null, message: null, ...overrides,
});

const folder = (root: string, lastThreadId: string | null = null) => ({
  root, name: root.split('/').pop()!, pinned: false, addedAt: '2026-10-09T10:00:00.000Z', lastOpenedAt: '2026-10-09T10:00:00.000Z',
  exists: true, threadCount: 1, instructions: null, spaceId: 'space_general', lastThreadId,
  index: { state: 'indexed', filesTotal: 10, filesIndexed: 10, passagesTotal: 100, passagesEmbedded: 100, bytes: 1, indexRoot: null, etaSeconds: null, message: null },
});

function emit(payload: FolderIndexStatus) {
  act(() => {
    for (const handler of mocks.listeners.get(INDEX_STATUS_EVENT) ?? []) handler({ payload });
  });
}

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listeners.clear();
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  mocks.api.explorerFoldersList.mockResolvedValue({ ok: true, data: { home: '/Users/me', folders: [folder(ROOT)] } });
});

describe('index status in the query cache', () => {
  it('one listener feeds both the open folder and the folders list', async () => {
    renderHook(() => useExplorerIndexStatusEvents(), { wrapper });
    const open = renderHook(() => useExplorerIndexStatus(ROOT), { wrapper });
    const list = renderHook(() => useExplorerIndexStatuses([ROOT, OTHER], 0), { wrapper });
    await waitFor(() => expect(mocks.listeners.get(INDEX_STATUS_EVENT)?.size).toBe(1));
    expect(open.result.current).toBeNull();

    emit(status());
    emit(status({ root: OTHER, indexRoot: OTHER, state: 'ready' }));
    await waitFor(() => expect(open.result.current).toEqual(status()));
    await waitFor(() => expect(list.result.current.get(ROOT)).toEqual(status()));
    expect(list.result.current.get(OTHER)?.state).toBe('ready');
    expect(client.getQueryData(explorerKeys.indexStatus(ROOT))).toEqual(status());
  });

  it('the folders list ignores a status older than its own read', () => {
    client.setQueryData(explorerKeys.indexStatus(ROOT), status());
    const updatedAt = client.getQueryState(explorerKeys.indexStatus(ROOT))!.dataUpdatedAt;
    const { result } = renderHook(() => useExplorerIndexStatuses([ROOT], updatedAt + 1), { wrapper });
    expect(result.current.size).toBe(0);
  });

  it('keeps an event that arrived while an index command was in flight', async () => {
    let reply: (value: unknown) => void = () => {};
    mocks.api.explorerIndexOpen.mockReturnValue(new Promise((resolve) => (reply = resolve)));
    renderHook(() => useExplorerIndexStatusEvents(), { wrapper });
    const commands = renderHook(() => useExplorerIndexCommands(), { wrapper });
    await waitFor(() => expect(mocks.listeners.get(INDEX_STATUS_EVENT)?.size).toBe(1));

    let opened: Promise<void> = Promise.resolve();
    act(() => { opened = commands.result.current.open(ROOT); });
    emit(status({ state: 'ready', passagesEmbedded: 100 }));
    await act(async () => {
      reply({ ok: true, data: status({ state: 'scanning' }) });
      await opened;
    });
    expect(client.getQueryData<FolderIndexStatus>(explorerKeys.indexStatus(ROOT))?.state).toBe('ready');
  });

  it('shows a failed command the way a failed run is shown', async () => {
    mocks.api.explorerIndexRebuild.mockResolvedValue({ ok: false, error: 'disk full' });
    const commands = renderHook(() => useExplorerIndexCommands(), { wrapper });
    await act(() => commands.result.current.rebuild(ROOT, false));
    expect(client.getQueryData(explorerKeys.indexStatus(ROOT))).toMatchObject({ state: 'error', message: 'disk full' });
  });

  it('closing the folder forgets its live status', async () => {
    mocks.api.explorerIndexClose.mockResolvedValue({ ok: true, data: undefined });
    client.setQueryData(explorerKeys.indexStatus(ROOT), status());
    const commands = renderHook(() => useExplorerIndexCommands(), { wrapper });
    await act(() => commands.result.current.close(ROOT));
    expect(client.getQueryData(explorerKeys.indexStatus(ROOT))).toBeUndefined();
  });
});

describe('a folder\'s last thread', () => {
  it('is written to the folder\'s row and shown in the cached list at once', async () => {
    client.setQueryData(explorerKeys.folders, { home: '/Users/me', folders: [folder(ROOT, 'thread-a')] });
    mocks.api.explorerFolderSetLastThread.mockResolvedValue({ ok: true, data: undefined });
    const { result } = renderHook(() => useExplorerFolderMutations(), { wrapper });
    await act(() => result.current.setLastThread.mutateAsync({ root: ROOT, conversationId: 'thread-b' }));
    expect(mocks.api.explorerFolderSetLastThread).toHaveBeenCalledWith(ROOT, 'thread-b');
    expect(client.getQueryData<{ folders: Array<{ lastThreadId: string | null }> }>(explorerKeys.folders)?.folders[0].lastThreadId).toBe('thread-b');
    expect(mocks.api.explorerFoldersList).not.toHaveBeenCalled();
  });

  it('refreshes the list when the folder was not listed yet', async () => {
    client.setQueryData(explorerKeys.folders, { home: '/Users/me', folders: [] });
    mocks.api.explorerFolderSetLastThread.mockResolvedValue({ ok: true, data: undefined });
    mocks.api.explorerFoldersList.mockResolvedValue({ ok: true, data: { home: '/Users/me', folders: [folder(ROOT, 'thread-a')] } });
    const { result } = renderHook(() => useExplorerFolderMutations(), { wrapper });
    await act(() => result.current.setLastThread.mutateAsync({ root: ROOT, conversationId: 'thread-a' }));
    expect(client.getQueryState(explorerKeys.folders)?.isInvalidated).toBe(true);
  });
});
