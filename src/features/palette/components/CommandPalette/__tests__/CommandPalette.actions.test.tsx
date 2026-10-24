import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OPEN_PALETTE_EVENT } from '@/features/palette/hooks/useCommandPalette';
import { usePaletteCommandsStore } from '@/features/palette/stores/paletteCommandsStore';
import { VaultAPI } from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';
import { logger } from '@/utils/logger';

import { CommandPalette } from '../CommandPalette';

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

// cmdk measures its list and scrolls the active item; jsdom has neither.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;
Element.prototype.scrollIntoView ??= () => {};

function Location() {
  const location = useLocation();
  return <output aria-label="location">{location.pathname + location.search}</output>;
}

function renderPalette() {
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })}>
      <MemoryRouter initialEntries={['/home']}>
        <CommandPalette />
        <Location />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

async function open() {
  act(() => { window.dispatchEvent(new Event(OPEN_PALETTE_EVENT)); });
  return screen.findByRole('dialog', { name: 'Command palette' });
}

const location = () => screen.getByRole('status', { name: 'location' }).textContent;
const toasts = () => useToastStore.getState().toasts.map(({ type, title, message }) => ({ type, title, message }));
const choose = (label: string) => fireEvent.click(screen.getByText(label));

beforeEach(() => {
  useToastStore.setState({ toasts: [] });
  usePaletteCommandsStore.setState({ commands: new Map() });
  localStorage.removeItem('lattice-command-palette');
  localStorage.removeItem('ingestHub.lastTab');
  api.selectMultipleFiles = vi.fn().mockResolvedValue([]);
  api.selectFolder = vi.fn().mockResolvedValue(null);
  api.indexFile = vi.fn().mockResolvedValue({ ok: true, data: null });
  api.startIndexing = vi.fn().mockResolvedValue({ ok: true, data: null });
  api.getDocument = vi.fn().mockResolvedValue({ ok: true, data: { filePath: '/library/cells.pdf' } });
  api.openFile = vi.fn().mockResolvedValue({ ok: true, data: null });
  api.searchDocuments = vi.fn().mockResolvedValue({ ok: true, data: [] });
  vi.spyOn(logger, 'error').mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

describe('CommandPalette actions', () => {
  it.each([
    ['Library', '/files'],
    ['Chat', '/chat'],
    ['Flashcards', '/studio?section=flashcards'],
    ['New learning program', '/studio?new=1'],
    ['New journal entry', '/journals?new=1'],
  ])('goes to %s and closes', async (label, path) => {
    renderPalette();
    await open();
    choose(label);
    expect(location()).toBe(path);
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it.each([
    ['Add web page', 'single-url'],
    ['Import', 'files'],
  ])('%s opens the import screen on its tab', async (label, tab) => {
    renderPalette();
    await open();
    choose(label);
    expect(location()).toBe('/ingest');
    expect(localStorage.getItem('ingestHub.lastTab')).toBe(tab);
  });

  it('still imports when the tab preference cannot be saved', async () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('quota'); });
    renderPalette();
    await open();
    choose('Import');
    expect(location()).toBe('/ingest');
  });

  it('adds each chosen file and reports how many failed', async () => {
    api.selectMultipleFiles.mockResolvedValue(['/notes/a.md', '/notes/b.md']);
    api.indexFile.mockResolvedValueOnce({ ok: false, error: 'unsupported format' }).mockResolvedValueOnce({ ok: true, data: null });
    renderPalette();
    await open();
    choose('Add files');
    await waitFor(() => expect(toasts()).toEqual([{ type: 'error', title: "Couldn't add 1 of 2 files", message: 'unsupported format' }]));
    expect(api.indexFile).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('names a single failed file in the singular', async () => {
    api.selectMultipleFiles.mockResolvedValue(['/notes/a.md']);
    api.indexFile.mockResolvedValue({ ok: false, error: 'locked' });
    renderPalette();
    await open();
    choose('Add files');
    await waitFor(() => expect(toasts()).toEqual([{ type: 'error', title: "Couldn't add 1 of 1 file", message: 'locked' }]));
  });

  it('does nothing when the file picker is cancelled, and reports a picker failure', async () => {
    renderPalette();
    await open();
    choose('Add files');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(api.indexFile).not.toHaveBeenCalled();
    expect(toasts()).toEqual([]);

    api.selectMultipleFiles.mockRejectedValue(new Error('dialog unavailable'));
    await open();
    choose('Add files');
    await waitFor(() => expect(toasts()).toEqual([{ type: 'error', title: "Couldn't add files", message: 'dialog unavailable' }]));
  });

  it('adds a chosen folder and reports a failed one', async () => {
    renderPalette();
    await open();
    choose('Add folder');
    await waitFor(() => expect(api.selectFolder).toHaveBeenCalled());
    expect(api.startIndexing).not.toHaveBeenCalled();

    api.selectFolder.mockResolvedValue('/recipes');
    api.startIndexing.mockResolvedValue({ ok: false, error: 'not readable' });
    await open();
    choose('Add folder');
    await waitFor(() => expect(toasts()).toEqual([{ type: 'error', title: "Couldn't add this folder", message: 'not readable' }]));
    expect(api.startIndexing).toHaveBeenCalledWith('/recipes', true);
  });

  it('reports a folder picker failure', async () => {
    api.selectFolder.mockRejectedValue(new Error('permission denied'));
    renderPalette();
    await open();
    choose('Add folder');
    await waitFor(() => expect(toasts()).toEqual([{ type: 'error', title: "Couldn't add this folder", message: 'permission denied' }]));
  });

  it('opens the keyboard shortcuts in place of the palette', async () => {
    renderPalette();
    await open();
    choose('Keyboard shortcuts');
    expect(await screen.findByText('Everywhere')).toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: 'Command palette' })).not.toBeInTheDocument();
  });

  it('opens a search result, then offers it and the query as recents', async () => {
    api.searchDocuments.mockResolvedValue({
      ok: true,
      data: [{ id: 'doc-1', title: 'Cell cycle', path: null, score: 0.8, content: 'Mitosis has four phases.', metadata: { filename: 'cells.pdf', path: '/library/cells.pdf' } }],
    });
    renderPalette();
    await open();
    fireEvent.change(screen.getByPlaceholderText('Search, or type a command'), { target: { value: 'mitosis' } });
    expect(await screen.findByText('Mitosis has four phases.')).toBeInTheDocument();
    expect(screen.getByText('0.80')).toBeInTheDocument();
    choose('cells.pdf');
    await waitFor(() => expect(api.openFile).toHaveBeenCalledWith('/library/cells.pdf'));

    await open();
    const searches = screen.getByText('Recent searches').closest<HTMLElement>('[cmdk-group]')!;
    expect(within(searches).getByText('mitosis')).toBeInTheDocument();
    const documents = screen.getByText('Recent documents').closest<HTMLElement>('[cmdk-group]')!;
    expect(within(documents).getByText('cells.pdf')).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem('lattice-command-palette')!).recentDocuments[0]).toMatchObject({ id: 'doc-1', label: 'cells.pdf' });
  });

  it('logs a search result that has no file or fails to open', async () => {
    api.searchDocuments.mockResolvedValue({
      ok: true,
      data: [
        { id: 'doc-2', title: 'Saffron', path: '/library/saffron.md', score: 0.5, content: '', metadata: {} },
        { id: 'doc-3', title: 'Pathless', path: null, score: 0.4, content: '', metadata: {} },
      ],
    });
    api.openFile.mockResolvedValue({ ok: false, error: 'moved' });
    renderPalette();
    await open();
    fireEvent.change(screen.getByPlaceholderText('Search, or type a command'), { target: { value: 'saffron' } });
    choose(await screen.findByText('Saffron').then((node) => node.textContent!));
    await waitFor(() => expect(logger.error).toHaveBeenCalledWith('Failed to open file:', { error: 'moved' }));

    await open();
    fireEvent.change(screen.getByPlaceholderText('Search, or type a command'), { target: { value: 'pathless' } });
    choose(await screen.findByText('Pathless').then((node) => node.textContent!));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(api.openFile).toHaveBeenCalledTimes(1);
  });

  it('reruns a recent search on the search screen and can clear them', async () => {
    localStorage.setItem('lattice-command-palette', JSON.stringify({ recentSearches: [{ id: 's1', label: 'photosynthesis', timestamp: 1 }], recentDocuments: [] }));
    renderPalette();
    await open();
    choose('photosynthesis');
    expect(location()).toBe('/search');

    await open();
    choose('Clear recent searches');
    expect(screen.queryByText('Recent searches')).not.toBeInTheDocument();
  });

  it('opens a recent document through its saved file path', async () => {
    localStorage.setItem('lattice-command-palette', JSON.stringify({ recentSearches: [], recentDocuments: [{ id: 'doc-1', label: 'Cell cycle', timestamp: 1 }] }));
    renderPalette();
    await open();
    choose('Cell cycle');
    await waitFor(() => expect(api.openFile).toHaveBeenCalledWith('/library/cells.pdf'));
    expect(api.getDocument).toHaveBeenCalledWith('doc-1');
  });

  it.each([
    ['metadata is missing', () => api.getDocument.mockResolvedValue({ ok: false, error: 'gone' }), 'Failed to retrieve document metadata'],
    ['the file will not open', () => api.openFile.mockResolvedValue({ ok: false, error: 'moved' }), 'Failed to open document'],
    ['the lookup throws', () => api.getDocument.mockRejectedValue(new Error('ipc down')), 'Unexpected error opening recent document'],
  ])('logs a recent document whose %s', async (_case, arrange, logged) => {
    arrange();
    localStorage.setItem('lattice-command-palette', JSON.stringify({ recentSearches: [], recentDocuments: [{ id: 'doc-1', label: 'Cell cycle', timestamp: 1 }] }));
    renderPalette();
    await open();
    choose('Cell cycle');
    await waitFor(() => expect(logger.error).toHaveBeenCalledWith(logged, expect.objectContaining({ docId: 'doc-1' })));
  });

  it('does not open a recent document that has no file', async () => {
    api.getDocument.mockResolvedValue({ ok: true, data: { filePath: '' } });
    localStorage.setItem('lattice-command-palette', JSON.stringify({ recentSearches: [], recentDocuments: [{ id: 'doc-1', label: 'Cell cycle', timestamp: 1 }] }));
    renderPalette();
    await open();
    choose('Cell cycle');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(api.openFile).not.toHaveBeenCalled();
  });

  it('runs commands registered by the current screen and hides disabled ones', async () => {
    const run = vi.fn();
    const failing = vi.fn().mockRejectedValue(new Error('nothing selected'));
    usePaletteCommandsStore.getState().register([
      { id: 'journal.synthesize', label: 'Synthesize this week', group: 'Journal', description: 'Summarise the pages', run },
      { id: 'journal.capture', label: 'Capture quote', run: failing },
      { id: 'journal.hidden', label: 'Hidden verb', enabled: false, run },
    ]);
    renderPalette();
    await open();
    const headings = [...document.querySelectorAll('[cmdk-group-heading]')].map((heading) => heading.textContent);
    expect(headings).toEqual(expect.arrayContaining(['Journal', 'Actions']));
    expect(screen.getByText('Summarise the pages')).toBeInTheDocument();
    expect(screen.queryByText('Hidden verb')).not.toBeInTheDocument();
    choose('Synthesize this week');
    expect(run).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

    await open();
    choose('Capture quote');
    await waitFor(() => expect(logger.error).toHaveBeenCalledWith('Palette command failed:', expect.objectContaining({ id: 'journal.capture' })));
  });

  it('toggles with Ctrl+K and closes on Escape', async () => {
    renderPalette();
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'K', metaKey: true });
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true, shiftKey: true });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    await screen.findByRole('dialog');
    fireEvent.keyDown(window, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('ignores unreadable saved recents', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    localStorage.setItem('lattice-command-palette', '{broken');
    renderPalette();
    await open();
    expect(screen.queryByText('Recent searches')).not.toBeInTheDocument();
    expect(console.error).toHaveBeenCalledWith('Failed to load command palette state:', expect.any(SyntaxError));
  });
});
