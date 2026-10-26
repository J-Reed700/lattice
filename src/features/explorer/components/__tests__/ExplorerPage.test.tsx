import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';


import { useExplorerStore } from '@/features/explorer/stores/explorerStore';
import VaultAPI from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';

import { ExplorerPage } from '../ExplorerPage';

const mocks = vi.hoisted(() => ({
  resolveExplorerRoot: vi.fn(),
  index: { open: vi.fn(), close: vi.fn(), rebuild: vi.fn() },
  setSettings: { mutateAsync: vi.fn() },
  foldersList: vi.fn(),
  loadSpaces: vi.fn(),
  fetchDownloadedModels: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
vi.mock('@/features/explorer/api/queries', () => ({
  explorerFoldersQueryOptions: () => ({ queryKey: ['explorer', 'folders'], queryFn: mocks.foldersList }),
  resolveExplorerRoot: mocks.resolveExplorerRoot,
  useExplorerFolderMutations: () => ({ setSettings: mocks.setSettings }),
  useExplorerIndexCommands: () => mocks.index,
  useExplorerIndexStatus: () => null,
  useExplorerIndexStatusEvents: () => undefined,
}));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ fetchDownloadedModels: mocks.fetchDownloadedModels }) }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: { loadSpaces: () => void }) => unknown) => select({ loadSpaces: mocks.loadSpaces }),
}));
vi.mock('../../hooks/useExplorerThread', () => ({ useExplorerThread: () => ({ threads: [] }) }));
vi.mock('../ExplorerChat', () => ({ ExplorerChat: () => <div>chat column</div> }));
vi.mock('../ExplorerFileView', () => ({ ExplorerFileView: ({ root }: { root: string }) => <div>file view of {root}</div> }));
vi.mock('../ExplorerTree', () => ({ ExplorerTree: () => <div>folder tree</div> }));
vi.mock('../ExplorerSearch', () => ({
  ExplorerSearch: ({ onActiveChange }: { onActiveChange: (active: boolean) => void }) => (
    <button type="button" onClick={() => onActiveChange(true)}>Start searching</button>
  ),
}));
vi.mock('../ExplorerStart', () => ({
  ExplorerStart: ({ opening, onChoose, onOpen }: { opening: string | null; onChoose: (path?: string) => void; onOpen: (folder: { root: string; name: string }) => void }) => (
    <div>
      <p>Opening: {opening ?? 'nothing'}</p>
      <button type="button" onClick={() => onChoose('/home')}>Choose a folder</button>
      <button type="button" onClick={() => onOpen({ root: '/notes/history', name: 'My history notes' })}>Open saved folder</button>
    </div>
  ),
}));
vi.mock('../ScopeBar', () => ({
  ScopeBar: (props: { onClose: () => void; closing: boolean; onRebuildIndex: () => void; onRetryIndex: () => void; onSettings: () => void; onToggleTree: () => void; treeHidden: boolean }) => (
    <div>
      <button type="button" onClick={props.onClose} disabled={props.closing}>Close folder</button>
      <button type="button" onClick={props.onRebuildIndex}>Rebuild index</button>
      <button type="button" onClick={props.onRetryIndex}>Retry index</button>
      <button type="button" onClick={props.onSettings}>Folder settings</button>
      <button type="button" onClick={props.onToggleTree}>{props.treeHidden ? 'Show tree' : 'Hide tree'}</button>
    </div>
  ),
}));
vi.mock('../FolderSettingsDialog', () => ({
  FolderSettingsDialog: ({ folder, onCancel, onSave }: { folder: { root: string }; onCancel: () => void; onSave: (instructions: string, spaceId: string) => Promise<boolean> }) => (
    <div role="dialog" aria-label="Folder settings">
      <p>Settings for {folder.root}</p>
      <button type="button" onClick={() => void onSave('Cite page numbers', 'history-space')}>Save settings</button>
      <button type="button" onClick={onCancel}>Cancel settings</button>
    </div>
  ),
}));

const ROOT = { root: '/notes/history', name: 'history' };
const toasts = () => useToastStore.getState().toasts.map(({ title, message }) => ({ title, message }));
const click = (name: string) => fireEvent.click(screen.getByRole('button', { name }));

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><ExplorerPage /></QueryClientProvider>);
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  useToastStore.setState({ toasts: [] });
  useExplorerStore.setState({ root: null });
  mocks.index.open.mockResolvedValue(undefined);
  mocks.index.close.mockResolvedValue(undefined);
  mocks.index.rebuild.mockResolvedValue(undefined);
  mocks.resolveExplorerRoot.mockResolvedValue({ ok: true, data: { root: '/notes/history', name: 'history' } });
  mocks.foldersList.mockResolvedValue({ home: null, folders: [{ root: '/notes/history', name: 'history' }] });
  mocks.setSettings.mutateAsync.mockResolvedValue(undefined);
});
afterEach(() => vi.restoreAllMocks());

describe('ExplorerPage without a folder', () => {
  it('loads what the chat needs and opens a picked folder', async () => {
    vi.mocked(open).mockResolvedValue('/notes/history');
    renderPage();
    expect(mocks.loadSpaces).toHaveBeenCalled();
    expect(mocks.fetchDownloadedModels).toHaveBeenCalled();
    click('Choose a folder');
    expect(open).toHaveBeenCalledWith({ directory: true, multiple: false, defaultPath: '/home' });
    expect(await screen.findByText('file view of /notes/history')).toBeInTheDocument();
    expect(mocks.resolveExplorerRoot).toHaveBeenCalledWith('/notes/history', VaultAPI);
    expect(mocks.index.open).toHaveBeenCalledWith('/notes/history', expect.any(Function));
  });

  it('does nothing when the picker is cancelled', async () => {
    vi.mocked(open).mockResolvedValue(null);
    renderPage();
    click('Choose a folder');
    await waitFor(() => expect(open).toHaveBeenCalled());
    expect(mocks.resolveExplorerRoot).not.toHaveBeenCalled();
  });

  it('keeps a saved folder’s own name and shows which folder is opening', async () => {
    let finish!: (value: unknown) => void;
    mocks.resolveExplorerRoot.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    renderPage();
    click('Open saved folder');
    expect(screen.getByText('Opening: /notes/history')).toBeInTheDocument();
    await act(async () => { finish({ ok: true, data: { root: '/notes/history', name: 'history' } }); });
    expect(useExplorerStore.getState().root).toEqual({ root: '/notes/history', name: 'My history notes' });
  });

  it('says why a folder could not be opened', async () => {
    mocks.resolveExplorerRoot.mockResolvedValue({ ok: false, error: 'Not a directory' });
    renderPage();
    click('Open saved folder');
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't open that folder", message: 'Not a directory' }]));
    expect(screen.getByText('Opening: nothing')).toBeInTheDocument();
    expect(useExplorerStore.getState().root).toBeNull();
  });
});

describe('ExplorerPage with a folder open', () => {
  beforeEach(() => useExplorerStore.setState({ root: ROOT }));

  it('closes the folder once its index has closed', async () => {
    let finish!: () => void;
    mocks.index.close.mockReturnValue(new Promise<void>((resolve) => { finish = resolve; }));
    renderPage();
    click('Close folder');
    expect(screen.getByRole('button', { name: 'Close folder' })).toBeDisabled();
    expect(mocks.index.close).toHaveBeenCalledWith('/notes/history');
    await act(async () => { finish(); });
    expect(useExplorerStore.getState().root).toBeNull();
    expect(screen.getByRole('button', { name: 'Choose a folder' })).toBeInTheDocument();
  });

  it('rebuilds or retries the index', () => {
    renderPage();
    click('Rebuild index');
    expect(mocks.index.rebuild).toHaveBeenLastCalledWith('/notes/history', false);
    click('Retry index');
    expect(mocks.index.rebuild).toHaveBeenLastCalledWith('/notes/history', true);
  });

  it('hides the tree from the bar or with Cmd+\\ and remembers it', () => {
    renderPage();
    const tree = screen.getByRole('complementary', { name: 'Folder tree' });
    expect(tree).not.toHaveClass('hidden');
    click('Hide tree');
    expect(tree).toHaveClass('hidden');
    expect(localStorage.getItem('explorer.treeHidden')).toBe('1');
    expect(screen.queryByRole('separator', { name: 'Resize folder tree' })).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: '\\', metaKey: true });
    expect(tree).not.toHaveClass('hidden');
    fireEvent.keyDown(window, { key: '\\', ctrlKey: true, shiftKey: true });
    expect(tree).not.toHaveClass('hidden');
    fireEvent.keyDown(window, { key: '\\', ctrlKey: true });
    expect(tree).toHaveClass('hidden');
  });

  it('hides the tree listing while a search shows its results', () => {
    renderPage();
    const listing = screen.getByText('folder tree').parentElement!;
    expect(listing).not.toHaveClass('hidden');
    click('Start searching');
    expect(listing).toHaveClass('hidden');
  });

  it('opens with saved widths and saves new ones', () => {
    localStorage.setItem('explorer.treeWidth', '300');
    localStorage.setItem('explorer.chatWidth', 'wide');
    vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} unobserve() {} });
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1600);
    renderPage();
    const tree = screen.getByRole('separator', { name: 'Resize folder tree' });
    const chat = screen.getByRole('separator', { name: 'Resize chat' });
    expect(tree).toHaveAttribute('aria-valuenow', '300');
    expect(chat).toHaveAttribute('aria-valuenow', '440');
    fireEvent.keyDown(tree, { key: 'ArrowRight' });
    expect(localStorage.getItem('explorer.treeWidth')).toBe('324');
    fireEvent.keyDown(chat, { key: 'ArrowLeft' });
    expect(localStorage.getItem('explorer.chatWidth')).toBe('464');
    fireEvent.doubleClick(tree);
    fireEvent.doubleClick(chat);
    expect(localStorage.getItem('explorer.treeWidth')).toBe('240');
    expect(localStorage.getItem('explorer.chatWidth')).toBe('440');
    vi.unstubAllGlobals();
  });

  it('keeps room for the file view in a narrow window', () => {
    const observe = vi.fn();
    const disconnect = vi.fn();
    vi.stubGlobal('ResizeObserver', class { observe = observe; disconnect = disconnect; unobserve() {} });
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(900);
    const { unmount } = renderPage();
    expect(observe).toHaveBeenCalled();
    expect(screen.getByRole('separator', { name: 'Resize chat' })).toHaveAttribute('aria-valuenow', '420');
    expect(screen.getByRole('separator', { name: 'Resize folder tree' })).toHaveAttribute('aria-valuenow', '160');
    unmount();
    expect(disconnect).toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it('opens the folder’s settings read fresh, and saves them', async () => {
    renderPage();
    click('Folder settings');
    expect(await screen.findByRole('dialog', { name: 'Folder settings' })).toHaveTextContent('Settings for /notes/history');
    click('Save settings');
    await waitFor(() => expect(mocks.setSettings.mutateAsync).toHaveBeenCalledWith({ root: '/notes/history', instructions: 'Cite page numbers', spaceId: 'history-space' }));
    expect(toasts()).toEqual([]);
    click('Cancel settings');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('reports settings that could not be saved', async () => {
    mocks.setSettings.mutateAsync.mockRejectedValue(new Error('space was archived'));
    renderPage();
    click('Folder settings');
    await screen.findByRole('dialog', { name: 'Folder settings' });
    click('Save settings');
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't save this folder's settings", message: 'space was archived' }]));
  });

  it.each([
    [() => mocks.foldersList.mockResolvedValue({ home: null, folders: [] }), 'It isn’t in Your folders yet. Try again in a moment.'],
    [() => mocks.foldersList.mockRejectedValue('list unavailable'), 'list unavailable'],
  ])('reports settings that could not be loaded (%#)', async (arrange, message) => {
    arrange();
    renderPage();
    click('Folder settings');
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't load this folder's settings", message }]));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
