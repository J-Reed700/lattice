import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useExplorerIndexStatusEvents } from '@/features/explorer/api/queries';
import type { FolderIndexStatus } from '@/stores/explorerStore';
import { folderBuildFixture } from '@/tests/fixtures/jobs';

import { describeFolderChip, openedAgo } from '../ExplorerFolders';
import { ExplorerStart } from '../ExplorerStart';

import type { ExplorerFolder, FolderIndexSummary } from '../useExplorerFolders';

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  rename: vi.fn(),
  setPinned: vi.fn(),
  deleteIndex: vi.fn(),
  remove: vi.fn(),
  saveSettings: vi.fn(),
  spaces: vi.fn(),
  apiCall: vi.fn(),
  listeners: new Set<(event: { payload: unknown }) => void>(),
}));
vi.mock('@/shared/ipc/transport', () => ({ apiCall: mocks.apiCall }));

vi.mock('@/lib/api', () => {
  const api = {
    explorerFoldersList: mocks.list,
    explorerFolderRename: mocks.rename,
    explorerFolderSetPinned: mocks.setPinned,
    explorerFolderDeleteIndex: mocks.deleteIndex,
    explorerFolderRemove: mocks.remove,
    explorerFolderSetSettings: mocks.saveSettings,
    listConversationSpaces: mocks.spaces,
  };
  return { VaultAPI: api, default: api };
});
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((_name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.listeners.add(handler);
    return Promise.resolve(() => mocks.listeners.delete(handler));
  }),
}));

const HOME = '/Users/mira';
const MB = 1024 * 1024;
const NOW = Date.now();
const ago = (minutes: number) => new Date(NOW - minutes * 60_000).toISOString();

function index(overrides: Partial<FolderIndexSummary> = {}): FolderIndexSummary {
  return {
    state: 'indexed',
    filesTotal: 1379,
    filesIndexed: 1379,
    passagesTotal: 10958,
    passagesEmbedded: 10958,
    bytes: 42 * MB,
    indexRoot: null,
    etaSeconds: null,
    message: null,
    ...overrides,
  };
}

function folder(name: string, overrides: Partial<ExplorerFolder> = {}): ExplorerFolder {
  return {
    root: `${HOME}/Code/${name}`,
    name,
    pinned: false,
    addedAt: ago(10_000),
    lastOpenedAt: ago(120),
    exists: true,
    threadCount: 0,
    index: index(),
    instructions: null,
    spaceId: 'space_general',
    lastThreadId: null,
    ...overrides,
  };
}

const CANOPY = folder('canopy-logger', { pinned: true, threadCount: 3, lastOpenedAt: ago(5) });
const MUSIC = folder('MusicVST', {
  lastOpenedAt: ago(60),
  index: index({ state: 'partial', passagesEmbedded: 4603, filesIndexed: 600, bytes: 18 * MB }),
});
const PLUGINS = folder('MusicVST/plugins', {
  name: 'plugins',
  lastOpenedAt: ago(90),
  index: index({ filesTotal: 212, bytes: 0, indexRoot: `${HOME}/Code/MusicVST` }),
});
const GONE = folder('old-site', { exists: false, lastOpenedAt: ago(60 * 24 * 3), index: index({ bytes: 2 * MB }) });
const FOLDERS = [CANOPY, MUSIC, PLUGINS, GONE];

function listed(folders: ExplorerFolder[]) {
  return { ok: true, data: { home: HOME, folders } };
}

/** The Explorer page's one status listener. */
function StatusEvents() {
  useExplorerIndexStatusEvents();
  return null;
}

function showStart(onOpen = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <StatusEvents />
      <ExplorerStart opening={null} onChoose={vi.fn()} onOpen={onOpen} />
    </QueryClientProvider>
  );
  return onOpen;
}

const rows = () => within(screen.getByRole('list')).getAllByRole('listitem');

/** The part of a row that opens its folder. */
function openButton(row: HTMLElement): HTMLButtonElement {
  const button = row.querySelector<HTMLButtonElement>('[data-folder-open]');
  if (!button) throw new Error('row has no open button');
  return button;
}

describe('the folders list in words', () => {
  it('names each row’s state on its chip', () => {
    expect(describeFolderChip(CANOPY).text).toBe('Indexed · 1,379 files');
    expect(describeFolderChip(MUSIC)).toMatchObject({ text: 'Paused at 42%', tone: 'paused', percent: 42 });
    expect(describeFolderChip(folder('x', { index: index({ state: 'indexing', passagesEmbedded: 3067, etaSeconds: 840 }) })).text).toBe(
      'Indexing 27% · ~14 min'
    );
    expect(describeFolderChip(folder('x', { index: index({ state: 'indexing', passagesEmbedded: 0, etaSeconds: null }) })).text).toBe('Indexing 0%');
    expect(describeFolderChip(folder('x', { index: index({ state: 'notIndexed' }) })).text).toBe('Not indexed');
    expect(describeFolderChip(folder('x', { index: index({ state: 'refused' }) })).text).toBe('Not indexed');
    expect(describeFolderChip(folder('x', { index: index({ state: 'tooLarge' }) })).text).toBe('Too large');
    expect(describeFolderChip(folder('x', { index: index({ state: 'error' }) })).text).toBe('Index error');
    expect(describeFolderChip(GONE)).toMatchObject({ text: 'Folder missing', tone: 'alarm' });
  });

  it('says when a folder was opened', () => {
    expect(openedAgo(ago(0), NOW)).toBe('just now');
    expect(openedAgo(ago(5), NOW)).toBe('5 min ago');
    expect(openedAgo(ago(120), NOW)).toBe('2 h ago');
    expect(openedAgo(ago(60 * 24), NOW)).toBe('yesterday');
    expect(openedAgo(ago(60 * 24 * 3), NOW)).toBe('3 days ago');
    expect(openedAgo('not a date', NOW)).toBe('');
  });
});

describe('Your folders', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listeners.clear();
    mocks.apiCall.mockResolvedValue({ ok: true, data: [] });
    localStorage.clear();
    mocks.list.mockResolvedValue(listed(FOLDERS));
    for (const action of [mocks.rename, mocks.setPinned, mocks.deleteIndex]) action.mockResolvedValue({ ok: true, data: undefined });
    mocks.remove.mockResolvedValue({ ok: true, data: 0 });
    mocks.spaces.mockResolvedValue({ ok: true, data: [] });
  });

  it('lists every folder with its state, threads, size and path from home', async () => {
    showStart();
    expect(await screen.findByRole('heading', { name: 'Your folders' })).toBeInTheDocument();
    expect(screen.getByText('4 folders · 62 MB of indexes')).toBeInTheDocument();
    const [canopy, music, plugins, gone] = rows();

    expect(canopy).toHaveTextContent('canopy-logger');
    expect(canopy).toHaveTextContent('Indexed · 1,379 files');
    expect(canopy).toHaveTextContent('~/Code/canopy-logger');
    expect(canopy).toHaveTextContent('3 threads');
    expect(canopy).toHaveTextContent('42 MB');
    expect(canopy).toHaveTextContent('opened 5 min ago');
    expect(within(canopy).getByRole('button', { name: 'Unpin canopy-logger' })).toHaveAttribute('aria-pressed', 'true');

    expect(music).toHaveTextContent('Paused at 42%');
    expect(plugins).toHaveTextContent('in ~/Code/MusicVST’s index');
    expect(gone).toHaveTextContent('Folder missing');
    expect(openButton(gone)).toBeDisabled();
    // The first-run explainer gives way to the list.
    expect(screen.queryByText(/stays until you delete it/)).not.toBeInTheDocument();
    expect(screen.queryByRole('searchbox', { name: 'Filter folders' })).not.toBeInTheDocument();
  });

  it('opens a folder from its row, and moves between rows with the arrow keys', async () => {
    const user = userEvent.setup();
    const onOpen = showStart();
    await screen.findByRole('heading', { name: 'Your folders' });
    const music = openButton(rows()[1]);
    await user.click(music);
    expect(onOpen).toHaveBeenCalledWith(MUSIC);

    music.focus();
    await user.keyboard('{ArrowDown}');
    expect(openButton(rows()[2])).toHaveFocus();
    await user.keyboard('{ArrowUp}{ArrowUp}');
    expect(openButton(rows()[0])).toHaveFocus();
  });

  it('pins a folder to the top at once', async () => {
    const user = userEvent.setup();
    let reply: (value: unknown) => void = () => {};
    mocks.setPinned.mockReturnValue(new Promise((resolve) => (reply = resolve)));
    showStart();
    await screen.findByRole('heading', { name: 'Your folders' });
    await user.click(screen.getByRole('button', { name: 'Pin MusicVST' }));
    expect(mocks.setPinned).toHaveBeenCalledWith(MUSIC.root, true);
    // Pinned, and opened less recently than canopy-logger: second.
    expect(rows()[1]).toHaveTextContent('MusicVST');
    expect(within(rows()[1]).getByRole('button', { name: 'Unpin MusicVST' })).toBeInTheDocument();

    mocks.list.mockResolvedValue(listed([CANOPY, { ...MUSIC, pinned: true }, PLUGINS, GONE]));
    await act(async () => reply({ ok: true, data: undefined }));
    await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(2));
  });

  it('renames in place: Enter keeps the name, Escape drops it', async () => {
    const user = userEvent.setup();
    showStart();
    await screen.findByRole('heading', { name: 'Your folders' });

    await user.click(screen.getByRole('button', { name: 'Actions for MusicVST' }));
    await user.click(await screen.findByRole('button', { name: 'Rename' }));
    const field = screen.getByRole('textbox', { name: 'Folder name' });
    expect(field).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(mocks.rename).not.toHaveBeenCalled();
    expect(screen.queryByRole('textbox', { name: 'Folder name' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Actions for MusicVST' }));
    await user.click(await screen.findByRole('button', { name: 'Rename' }));
    await user.clear(screen.getByRole('textbox', { name: 'Folder name' }));
    await user.keyboard('Synth engine{Enter}');
    expect(mocks.rename).toHaveBeenCalledWith(MUSIC.root, 'Synth engine');
  });

  it('deletes only a folder’s own index', async () => {
    const user = userEvent.setup();
    showStart();
    await screen.findByRole('heading', { name: 'Your folders' });

    await user.click(screen.getByRole('button', { name: 'Actions for plugins' }));
    const reused = await screen.findByRole('button', { name: /Delete index/ });
    expect(reused).toBeDisabled();
    expect(reused).toHaveTextContent('Uses ~/Code/MusicVST’s index');
    await user.keyboard('{Escape}');

    await user.click(screen.getByRole('button', { name: 'Actions for MusicVST' }));
    await user.click(await screen.findByRole('button', { name: /Delete index/ }));
    expect(mocks.deleteIndex).toHaveBeenCalledWith(MUSIC.root);
  });

  it('asks before removing, keeps the threads unless ticked, and leaves the folder on disk', async () => {
    const user = userEvent.setup();
    showStart();
    await screen.findByRole('heading', { name: 'Your folders' });

    await user.click(screen.getByRole('button', { name: 'Actions for canopy-logger' }));
    await user.click(await screen.findByRole('button', { name: 'Remove…' }));
    const dialog = await screen.findByRole('dialog', { name: 'Remove canopy-logger from Lattice?' });
    expect(dialog).toHaveTextContent('Its index (42 MB) is deleted from Lattice’s data.');
    expect(dialog).toHaveTextContent('The folder on disk isn\'t touched.');
    const threads = within(dialog).getByRole('checkbox', { name: /Also delete its 3 chat threads/ });
    expect(threads).not.toBeChecked();

    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(mocks.remove).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Actions for canopy-logger' }));
    await user.click(await screen.findByRole('button', { name: 'Remove…' }));
    const again = await screen.findByRole('dialog');
    await user.click(within(again).getByRole('checkbox', { name: /Also delete its 3 chat threads/ }));
    mocks.remove.mockResolvedValue({ ok: true, data: 3 });
    mocks.list.mockResolvedValue(listed([MUSIC, PLUGINS, GONE]));
    await user.click(within(again).getByRole('button', { name: 'Remove' }));
    expect(mocks.remove).toHaveBeenCalledWith(CANOPY.root, true);
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(screen.queryByText('canopy-logger')).not.toBeInTheDocument();
  });

  it('edits a folder’s system prompt and keeps its space', async () => {
    const user = userEvent.setup();
    const space = (id: string, name: string, sortOrder: number) => ({
      id, name, description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null,
      toolPreferencesJson: null, isArchived: false, sortOrder, createdAt: ago(1), updatedAt: ago(1),
    });
    mocks.spaces.mockResolvedValue({ ok: true, data: [space('space_music', 'Music', 1), space('space_general', 'General', 0)] });
    mocks.list.mockResolvedValue(listed([{ ...MUSIC, threadCount: 2, instructions: 'Old prompt' }, CANOPY]));
    mocks.saveSettings.mockResolvedValue({ ok: true, data: 0 });
    showStart();
    await screen.findByRole('heading', { name: 'Your folders' });

    await user.click(screen.getByRole('button', { name: 'Actions for MusicVST' }));
    await user.click(await screen.findByRole('button', { name: /Settings…/ }));
    const dialog = await screen.findByRole('dialog', { name: 'MusicVST settings' });
    expect(within(dialog).getByRole('combobox', { name: 'Space' })).toHaveTextContent('General');
    const prompt = within(dialog).getByRole('textbox', { name: 'System prompt' });
    expect(prompt).toHaveValue('Old prompt');
    const save = within(dialog).getByRole('button', { name: 'Save' });
    expect(save).toBeDisabled();

    await user.clear(prompt);
    await user.type(prompt, 'JUCE plugin, C++20.');
    await user.click(save);
    expect(mocks.saveSettings).toHaveBeenCalledWith(MUSIC.root, 'JUCE plugin, C++20.', 'space_general');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('keeps the settings open when saving fails', async () => {
    const user = userEvent.setup();
    mocks.saveSettings.mockResolvedValue({ ok: false, error: 'Space not found or archived: space_old' });
    showStart();
    await screen.findByRole('heading', { name: 'Your folders' });

    await user.click(screen.getByRole('button', { name: 'Actions for MusicVST' }));
    await user.click(await screen.findByRole('button', { name: /Settings…/ }));
    const dialog = await screen.findByRole('dialog', { name: 'MusicVST settings' });
    await user.type(within(dialog).getByRole('textbox', { name: 'System prompt' }), 'Anything');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenCalled());
    expect(screen.getByRole('dialog', { name: 'MusicVST settings' })).toBeInTheDocument();
  });

  it('offers no thread checkbox for a folder without threads', async () => {
    const user = userEvent.setup();
    showStart();
    await screen.findByRole('heading', { name: 'Your folders' });
    await user.click(screen.getByRole('button', { name: 'Actions for plugins' }));
    await user.click(await screen.findByRole('button', { name: 'Remove…' }));
    const dialog = await screen.findByRole('dialog', { name: 'Remove plugins from Lattice?' });
    expect(dialog).toHaveTextContent('It uses the index of ~/Code/MusicVST, which stays.');
    expect(within(dialog).queryByRole('checkbox')).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
    expect(mocks.remove).toHaveBeenCalledWith(PLUGINS.root, false);
  });

  it('filters a long list by name or path', async () => {
    const user = userEvent.setup();
    const many = Array.from({ length: 8 }, (_, n) => folder(`project-${n}`, { lastOpenedAt: ago(n) }));
    mocks.list.mockResolvedValue(listed([...many, folder('synth', { root: `${HOME}/Music/synth` })]));
    showStart();
    const filter = await screen.findByRole('searchbox', { name: 'Filter folders' });
    expect(rows()).toHaveLength(9);
    await user.type(filter, 'music');
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toHaveTextContent('synth');
    await user.clear(filter);
    await user.type(filter, 'nothing like it');
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
    expect(screen.getByText('No folders match “nothing like it”.')).toBeInTheDocument();
  });

  it('follows the open folder’s index live', async () => {
    mocks.list.mockResolvedValue(listed([MUSIC]));
    showStart();
    await screen.findByRole('heading', { name: 'Your folders' });
    await waitFor(() => expect(mocks.listeners.size).toBe(1));
    const event: FolderIndexStatus = {
      root: MUSIC.root,
      indexRoot: MUSIC.root,
      state: 'indexing',
      filesTotal: 1379,
      filesIndexed: 700,
      passagesTotal: 10958,
      passagesEmbedded: 6575,
      passagesPerSecond: 20,
      etaSeconds: 220,
      message: null,
    };
    act(() => mocks.listeners.forEach((handler) => handler({ payload: folderBuildFixture(event) })));
    await waitFor(() => expect(rows()[0]).toHaveTextContent('Indexing 60% · ~4 min'));
    act(() => mocks.listeners.forEach((handler) => handler({ payload: folderBuildFixture({ ...event, state: 'ready', passagesEmbedded: 10958, etaSeconds: null }) })));
    await waitFor(() => expect(rows()[0]).toHaveTextContent('Indexed · 1,379 files'));
  });
});
