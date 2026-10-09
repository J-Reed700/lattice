import type { ReactElement } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render as renderBare, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { explorerKeys, INDEX_STATUS_EVENT } from '@/features/explorer/api/queries';
import { useExplorerStore, type FolderIndexStatus } from '@/stores/explorerStore';

import { describeIndexNotice, ExplorerIndexNotice } from '../ExplorerIndexNotice';
import { ExplorerPage } from '../ExplorerPage';
import { formatEta, formatRate, tildePath } from '../indexProgress';
import { describeIndexCounts, describeIndexStatus } from '../IndexStatusPill';
import { ScopeBar } from '../ScopeBar';

const mocks = vi.hoisted(() => ({
  indexOpen: vi.fn(),
  indexClose: vi.fn(),
  indexRebuild: vi.fn(),
  resolveRoot: vi.fn(),
  foldersList: vi.fn(),
  listeners: new Map<string, Set<(event: { payload: unknown }) => void>>(),
  conversations: {
    conversations: [] as { id: string; explorerRoot: string | null }[],
    activeConversationId: null as string | null,
  },
}));

vi.mock('@/lib/api', () => ({
  VaultAPI: {
    explorerIndexOpen: mocks.indexOpen,
    explorerIndexClose: mocks.indexClose,
    explorerIndexRebuild: mocks.indexRebuild,
    explorerResolveRoot: mocks.resolveRoot,
    explorerFoldersList: mocks.foldersList,
  },
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    const handlers = mocks.listeners.get(name) ?? new Set();
    handlers.add(handler);
    mocks.listeners.set(name, handlers);
    return Promise.resolve(() => handlers.delete(handler));
  }),
}));
// The page's other columns have their own tests; here they are placeholders.
vi.mock('../ExplorerChat', () => ({ ExplorerChat: () => <div /> }));
vi.mock('../ExplorerFileView', () => ({ ExplorerFileView: () => <div /> }));
vi.mock('../ExplorerSearch', () => ({ ExplorerSearch: () => <div /> }));
vi.mock('../ExplorerTree', () => ({ ExplorerTree: () => <div /> }));
vi.mock('../useExplorerThread', () => ({ useExplorerThread: () => ({}) }));
vi.mock('@/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ fetchDownloadedModels: vi.fn() }) }));
vi.mock('@/stores/conversationsStore', () => ({
  useConversationsStore: (select: (state: object) => unknown) => select({ loadSpaces: vi.fn(), ...mocks.conversations }),
}));

let client = new QueryClient();

/** Renders inside a fresh query cache, kept in `client` for the test to read and seed. */
function render(ui: ReactElement) {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const result = renderBare(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
  return { ...result, rerender: (next: ReactElement) => result.rerender(<QueryClientProvider client={client}>{next}</QueryClientProvider>) };
}

const ROOT = '/Users/me/project';
const PROJECT = { root: ROOT, name: 'project' };

function status(overrides: Partial<FolderIndexStatus> = {}): FolderIndexStatus {
  return {
    root: ROOT,
    indexRoot: ROOT,
    state: 'ready',
    filesTotal: 1379,
    filesIndexed: 1379,
    passagesTotal: 10958,
    passagesEmbedded: 10958,
    passagesPerSecond: null,
    etaSeconds: null,
    message: null,
    ...overrides,
  };
}

/** Sends a status event to everything listening, as the backend would. */
function emit(payload: FolderIndexStatus) {
  act(() => {
    for (const handler of mocks.listeners.get(INDEX_STATUS_EVENT) ?? []) handler({ payload });
  });
}

const INDEXING = status({ state: 'indexing', passagesEmbedded: 3067, filesIndexed: 489, passagesPerSecond: 12.4, etaSeconds: 840 });

describe('folder index progress in words', () => {
  it('formats time left for people', () => {
    expect(formatEta(0)).toBe('<1 min');
    expect(formatEta(59)).toBe('<1 min');
    expect(formatEta(170)).toBe('~3 min');
    expect(formatEta(840)).toBe('~14 min');
    expect(formatEta(3600)).toBe('~1 h');
    expect(formatEta(4200)).toBe('~1 h 10 min');
    expect(formatEta(3 * 3600 + 29)).toBe('~3 h');
  });

  it('says the rate and the home folder plainly', () => {
    expect(formatRate(12.44)).toBe('12 passages/s');
    expect(formatRate(3.14)).toBe('3.1 passages/s');
    expect(tildePath('/Users/me/Code/lattice', '/Users/me')).toBe('~/Code/lattice');
    expect(tildePath('/Users/me', '/Users/me')).toBe('~');
    expect(tildePath('/Users/meg/Code', '/Users/me')).toBe('/Users/meg/Code');
    expect(tildePath('/opt/x', null)).toBe('/opt/x');
  });

  it('gives the pill its words in each state', () => {
    expect(describeIndexStatus(status({ state: 'scanning', filesTotal: 1379 }))).toBe('Scanning · 1,379 files');
    expect(describeIndexStatus(status({ state: 'scanning', filesTotal: 0 }))).toBe('Scanning files…');
    expect(describeIndexStatus(INDEXING)).toBe('Indexing 27% · ~14 min left');
    expect(describeIndexStatus({ ...INDEXING, etaSeconds: null })).toBe('Indexing 27%');
    expect(describeIndexStatus({ ...INDEXING, passagesEmbedded: 10957 })).toBe('Indexing 99% · ~14 min left');
    expect(describeIndexStatus(status())).toBe('Indexed · 1,379 files');
    expect(describeIndexStatus(status({ state: 'tooLarge' }))).toBe('Too large to index · search uses text matching');
    expect(describeIndexStatus(status({ state: 'refused' }))).toBe('Not indexed (home folder)');
    expect(describeIndexStatus(status({ state: 'unavailable' }))).toBe('No embedding model');
    expect(describeIndexStatus(status({ state: 'error' }))).toBe('Index failed');
    expect(describeIndexCounts(INDEXING)).toBe('3,067 of 10,958 passages · 489 of 1,379 files');
    expect(describeIndexCounts(status({ state: 'scanning', filesTotal: 0, filesIndexed: 0, passagesTotal: 0, passagesEmbedded: 0 }))).toBeNull();
  });
});

describe('folder index status pill', () => {
  it('drives its bar by passages and opens to the counts, the rate and Rebuild', async () => {
    const user = userEvent.setup();
    const onRebuild = vi.fn();
    render(<ScopeBar root={PROJECT} onClose={vi.fn()} indexStatus={INDEXING} onRebuildIndex={onRebuild} />);
    const pill = screen.getByRole('button', { name: 'Folder index: Indexing 27% · ~14 min left' });
    expect(pill.querySelector('[style]')).toHaveStyle({ width: '27%' });
    expect(screen.queryByRole('button', { name: 'Retry' })).not.toBeInTheDocument();

    await user.click(pill);
    expect(await screen.findByText('3,067 of 10,958 passages · 489 of 1,379 files')).toBeInTheDocument();
    expect(screen.getByText('12 passages/s')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Rebuild index' }));
    expect(onRebuild).toHaveBeenCalledOnce();
  });

  it('offers Retry when the index failed', async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    render(<ScopeBar root={PROJECT} onClose={vi.fn()} indexStatus={status({ state: 'error', message: 'disk full' })} onRetryIndex={onRetry} />);
    expect(screen.getByRole('button', { name: 'Folder index: Index failed' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it('is absent until the backend has said', () => {
    render(<ScopeBar root={PROJECT} onClose={vi.fn()} indexStatus={null} />);
    expect(screen.queryByRole('button', { name: /Folder index/ })).not.toBeInTheDocument();
  });
});

describe('the chat column while the folder indexes', () => {
  beforeEach(() => {
    mocks.conversations.conversations = [
      { id: 'thread', explorerRoot: ROOT },
      { id: 'chat', explorerRoot: null },
    ];
    mocks.conversations.activeConversationId = 'thread';
    useExplorerStore.setState({ root: PROJECT });
  });

  it('says how far along the index is, and nothing once it is ready', async () => {
    expect(describeIndexNotice(INDEXING)).toBe('Indexing this folder · 27%');
    expect(describeIndexNotice(status({ state: 'scanning', filesTotal: 412 }))).toBe('Scanning this folder · 412 files');
    expect(describeIndexNotice(status())).toBeNull();
    expect(describeIndexNotice(null)).toBeNull();

    const { rerender } = render(<ExplorerIndexNotice />);
    act(() => { client.setQueryData(explorerKeys.indexStatus(ROOT), INDEXING); });
    expect(await screen.findByRole('status')).toHaveTextContent("Indexing this folder · 27% — search covers what's indexed so far");

    act(() => { client.setQueryData(explorerKeys.indexStatus(ROOT), status()); });
    rerender(<ExplorerIndexNotice />);
    await waitFor(() => expect(screen.queryByRole('status')).not.toBeInTheDocument());
  });

  it('stays out of a chat that is not this folder’s thread', () => {
    mocks.conversations.activeConversationId = 'chat';
    render(<ExplorerIndexNotice />);
    act(() => { client.setQueryData(explorerKeys.indexStatus(ROOT), INDEXING); });
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });
});

describe('ExplorerPage and the folder index', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listeners.clear();
    localStorage.clear();
    useExplorerStore.setState({ root: null });
    mocks.indexClose.mockResolvedValue({ ok: true, data: undefined });
    mocks.foldersList.mockResolvedValue({ ok: true, data: { home: '/Users/me', folders: [] } });
  });

  it('opens the index for the folder, follows its events, and closes it with the folder', async () => {
    const user = userEvent.setup();
    mocks.indexOpen.mockResolvedValue({ ok: true, data: status({ state: 'scanning', filesTotal: 0, filesIndexed: 0 }) });
    useExplorerStore.setState({ root: PROJECT });
    render(<ExplorerPage />);

    expect(mocks.indexOpen).toHaveBeenCalledWith(ROOT);
    expect(await screen.findByRole('button', { name: 'Folder index: Scanning files…' })).toBeInTheDocument();

    await waitFor(() => expect(mocks.listeners.get(INDEX_STATUS_EVENT)?.size).toBe(1));
    emit(status({ state: 'indexing', passagesEmbedded: 1200, passagesTotal: 4000 }));
    expect(await screen.findByRole('button', { name: 'Folder index: Indexing 30%' })).toBeInTheDocument();
    // Another folder's progress is not this one's.
    emit(status({ root: '/elsewhere', state: 'ready' }));
    expect(screen.getByRole('button', { name: 'Folder index: Indexing 30%' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Close folder' }));
    await waitFor(() => expect(useExplorerStore.getState().root).toBeNull());
    expect(mocks.indexClose).toHaveBeenCalledOnce();
    expect(mocks.indexClose.mock.invocationCallOrder[0]).toBeGreaterThan(mocks.indexOpen.mock.invocationCallOrder[0]);
  });

  it('says Closing… and holds the button while the index lets go', async () => {
    const user = userEvent.setup();
    let release: (value: unknown) => void = () => {};
    mocks.indexClose.mockReturnValue(new Promise((resolve) => (release = resolve)));
    mocks.indexOpen.mockResolvedValue({ ok: true, data: INDEXING });
    useExplorerStore.setState({ root: PROJECT });
    render(<ExplorerPage />);

    await user.click(screen.getByRole('button', { name: 'Close folder' }));
    const closing = screen.getByRole('button', { name: 'Closing…' });
    expect(closing).toBeDisabled();
    expect(closing).toHaveAttribute('aria-busy', 'true');
    await user.click(closing);
    expect(mocks.indexClose).toHaveBeenCalledOnce();
    expect(useExplorerStore.getState().root).toEqual(PROJECT);

    await act(async () => release({ ok: true, data: undefined }));
    await waitFor(() => expect(useExplorerStore.getState().root).toBeNull());
  });

  it('keeps an event that arrived while the open command was in flight', async () => {
    let reply: (value: unknown) => void = () => {};
    mocks.indexOpen.mockReturnValue(new Promise((resolve) => (reply = resolve)));
    useExplorerStore.setState({ root: PROJECT });
    render(<ExplorerPage />);
    await waitFor(() => expect(mocks.listeners.get(INDEX_STATUS_EVENT)?.size).toBe(1));
    emit(status({ filesTotal: 3, filesIndexed: 3 }));
    reply({ ok: true, data: status({ state: 'scanning', filesTotal: 0, filesIndexed: 0 }) });
    expect(await screen.findByRole('button', { name: 'Folder index: Indexed · 3 files' })).toBeInTheDocument();
  });

  it('starts on the choice alone when no folder has been picked yet', async () => {
    render(<ExplorerPage />);
    expect(await screen.findByRole('button', { name: /Choose a folder/ })).toBeInTheDocument();
    await waitFor(() => expect(mocks.foldersList).toHaveBeenCalledOnce());
    expect(screen.getByText(/stays until you delete it/)).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Your folders' })).not.toBeInTheDocument();
    expect(mocks.indexOpen).not.toHaveBeenCalled();
  });
});
