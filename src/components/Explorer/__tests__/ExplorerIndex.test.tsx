import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useExplorerStore, type FolderIndexStatus } from '@/stores/explorerStore';

import { ExplorerPage, INDEX_STATUS_EVENT } from '../ExplorerPage';
import { describeIndexStatus } from '../IndexStatusPill';
import { ScopeBar } from '../ScopeBar';

const mocks = vi.hoisted(() => ({
  indexOpen: vi.fn(),
  indexClose: vi.fn(),
  indexRebuild: vi.fn(),
  indexForget: vi.fn(),
  resolveRoot: vi.fn(),
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
}));

vi.mock('@/lib/api', () => ({
  VaultAPI: {
    explorerIndexOpen: mocks.indexOpen,
    explorerIndexClose: mocks.indexClose,
    explorerIndexRebuild: mocks.indexRebuild,
    explorerIndexForget: mocks.indexForget,
    explorerResolveRoot: mocks.resolveRoot,
  },
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(name, handler);
    return Promise.resolve(() => mocks.listeners.delete(name));
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
  useConversationsStore: (select: (state: { loadSpaces: () => void }) => unknown) => select({ loadSpaces: vi.fn() }),
}));

const ROOT = '/Users/me/project';
const PROJECT = { root: ROOT, name: 'project' };

function status(overrides: Partial<FolderIndexStatus> = {}): FolderIndexStatus {
  return { root: ROOT, indexRoot: ROOT, state: 'ready', filesTotal: 3100, filesIndexed: 3100, chunks: 9000, message: null, ...overrides };
}

describe('folder index status pill', () => {
  it('says where the index stands in each state', () => {
    expect(describeIndexStatus(status({ state: 'indexing', filesIndexed: 1240 }))).toBe('Indexing 1,240 / 3,100 files');
    expect(describeIndexStatus(status())).toBe('Indexed · 3,100 files');
    expect(describeIndexStatus(status({ state: 'tooLarge' }))).toBe('Too large to index · search uses text matching');
    expect(describeIndexStatus(status({ state: 'refused' }))).toBe('Not indexed (home folder)');
    expect(describeIndexStatus(status({ state: 'unavailable' }))).toBe('No embedding model');
    expect(describeIndexStatus(status({ state: 'error' }))).toBe('Index failed');
    expect(describeIndexStatus(status({ state: 'scanning' }))).toBe('Scanning files…');
  });

  it('shows progress while indexing and offers Rebuild in its menu', async () => {
    const user = userEvent.setup();
    const onRebuild = vi.fn();
    render(
      <ScopeBar root={PROJECT} onClose={vi.fn()} indexStatus={status({ state: 'indexing', filesIndexed: 1240 })} onRebuildIndex={onRebuild} />
    );
    const pill = screen.getByRole('button', { name: 'Folder index: Indexing 1,240 / 3,100 files' });
    expect(pill.querySelector('[style]')).toHaveStyle({ width: '40%' });
    expect(screen.queryByRole('button', { name: 'Retry' })).not.toBeInTheDocument();

    await user.click(pill);
    await user.click(await screen.findByRole('button', { name: 'Rebuild index' }));
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

describe('ExplorerPage and the folder index', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listeners.clear();
    localStorage.clear();
    useExplorerStore.setState({ root: null, recentRoots: [], indexStatus: null, indexEvents: 0 });
    mocks.indexClose.mockResolvedValue({ ok: true, data: undefined });
    mocks.indexForget.mockResolvedValue({ ok: true, data: undefined });
  });

  it('opens the index for the folder, follows its events, and closes it with the folder', async () => {
    const user = userEvent.setup();
    mocks.indexOpen.mockResolvedValue({ ok: true, data: status({ state: 'scanning', filesTotal: 0, filesIndexed: 0 }) });
    useExplorerStore.setState({ root: PROJECT });
    render(<ExplorerPage />);

    expect(mocks.indexOpen).toHaveBeenCalledWith(ROOT);
    expect(await screen.findByRole('button', { name: 'Folder index: Scanning files…' })).toBeInTheDocument();

    await waitFor(() => expect(mocks.listeners.has(INDEX_STATUS_EVENT)).toBe(true));
    mocks.listeners.get(INDEX_STATUS_EVENT)!({ payload: status({ state: 'indexing', filesIndexed: 12, filesTotal: 40 }) });
    expect(await screen.findByRole('button', { name: 'Folder index: Indexing 12 / 40 files' })).toBeInTheDocument();
    // Another folder's progress is not this one's.
    mocks.listeners.get(INDEX_STATUS_EVENT)!({ payload: status({ root: '/elsewhere', state: 'ready' }) });
    expect(screen.getByRole('button', { name: 'Folder index: Indexing 12 / 40 files' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Close folder' }));
    await waitFor(() => expect(useExplorerStore.getState().root).toBeNull());
    expect(mocks.indexClose).toHaveBeenCalledOnce();
    expect(mocks.indexClose.mock.invocationCallOrder[0]).toBeGreaterThan(mocks.indexOpen.mock.invocationCallOrder[0]);
  });

  it('keeps an event that arrived while the open command was in flight', async () => {
    let reply: (value: unknown) => void = () => {};
    mocks.indexOpen.mockReturnValue(new Promise((resolve) => (reply = resolve)));
    useExplorerStore.setState({ root: PROJECT });
    render(<ExplorerPage />);
    await waitFor(() => expect(mocks.listeners.has(INDEX_STATUS_EVENT)).toBe(true));
    mocks.listeners.get(INDEX_STATUS_EVENT)!({ payload: status({ filesTotal: 3, filesIndexed: 3 }) });
    reply({ ok: true, data: status({ state: 'scanning', filesTotal: 0, filesIndexed: 0 }) });
    expect(await screen.findByRole('button', { name: 'Folder index: Indexed · 3 files' })).toBeInTheDocument();
  });

  it('forgets a recent folder: its index, then its row', async () => {
    const user = userEvent.setup();
    useExplorerStore.setState({ root: null, recentRoots: [PROJECT, { root: '/Users/me/other', name: 'other' }] });
    render(<ExplorerPage />);
    expect(screen.getByText(/index lives in Lattice’s data|index lives in Lattice's data/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Forget project' }));
    expect(mocks.indexForget).toHaveBeenCalledWith(ROOT);
    await waitFor(() => expect(screen.queryByText(ROOT)).not.toBeInTheDocument());
    expect(screen.getByText('/Users/me/other')).toBeInTheDocument();
    expect(mocks.indexOpen).not.toHaveBeenCalled();
  });
});
