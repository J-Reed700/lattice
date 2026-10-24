import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { ListView } from './ListView';

const h = vi.hoisted(() => ({
  query: {} as Record<string, unknown>,
  store: {} as Record<string, any>,
}));

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count, estimateSize }: { count: number; estimateSize: (index: number) => number }) => ({
    getTotalSize: () => count * 50,
    getVirtualItems: () => Array.from({ length: count }, (_v, index) => ({ index, start: index * 50, size: estimateSize(index), key: `row-${index}` })),
  }),
}));
vi.mock('@/features/files/hooks/useLibraryDocumentsQuery', () => ({ useLibraryDocumentsQuery: () => h.query }));
vi.mock('@/features/files/stores/fileBrowserStore', () => ({
  useFileBrowserStore: (selector: (state: unknown) => unknown) => selector(h.store),
}));

const NOW = new Date(2026, 4, 15, 12);
const doc = (id: string, daysAgo: number) => ({
  id, fileName: `${id}.md`, filePath: `/notes/${id}.md`, fileType: 'md', category: 'document',
  modifiedAt: new Date(NOW.getTime() - daysAgo * 86_400_000).toISOString(), indexedAt: '', wordCount: 10,
}) as DocumentMetadata;

const docs = [doc('rye', 0), doc('spelt', 1), doc('emmer', 3), doc('einkorn', 10), doc('barley', 20), doc('millet', 90)];

const row = (name: string) => screen.getByRole('checkbox', { name: `Select ${name}` }).closest('[aria-selected]') as HTMLElement;

describe('ListView interactions', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(NOW);
    h.query = { documents: docs, filteredDocuments: docs, isLoading: false, error: null };
    h.store = {
      groupByDate: false,
      searchQuery: '',
      setSearchQuery: vi.fn(),
      selectedDocumentIds: new Set<string>(),
      selectFile: vi.fn(),
      toggleSelection: vi.fn(),
      clearSelection: vi.fn(),
      setFocusedDocument: vi.fn(),
    };
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('groups documents under date headings', () => {
    h.store.groupByDate = true;
    render(<ListView />);
    for (const heading of ['Today', 'Yesterday', 'This week', 'Last week', 'This month', 'Older']) {
      expect(screen.getByText(heading)).toBeInTheDocument();
    }
  });

  it('selects one row, toggles with a modifier, and selects a range with Shift', () => {
    render(<ListView />);
    fireEvent.click(row('spelt.md'));
    expect(h.store.clearSelection).toHaveBeenCalledTimes(1);
    expect(h.store.selectFile).toHaveBeenCalledWith('spelt');
    expect(h.store.setFocusedDocument).toHaveBeenCalledWith('spelt');
    fireEvent.click(row('emmer.md'), { metaKey: true });
    fireEvent.click(row('barley.md'), { ctrlKey: true });
    expect(h.store.toggleSelection.mock.calls).toEqual([['emmer'], ['barley']]);
    h.store.selectFile.mockClear();
    fireEvent.click(row('spelt.md'), { shiftKey: true });
    expect(h.store.selectFile.mock.calls.map(([id]: unknown[]) => id)).toEqual(['spelt', 'emmer', 'einkorn', 'barley']);
  });

  it('treats Shift without an anchor as a plain click', () => {
    render(<ListView />);
    fireEvent.click(row('emmer.md'), { shiftKey: true });
    expect(h.store.selectFile).toHaveBeenCalledWith('emmer');
  });

  it('opens, selects, renames, deletes and shows the menu for a row', () => {
    const props = { onFileOpen: vi.fn(), onContextMenu: vi.fn(), onRename: vi.fn(), onDelete: vi.fn() };
    h.store.selectedDocumentIds = new Set(['rye']);
    render(<ListView {...props} />);
    fireEvent.doubleClick(row('rye.md'));
    expect(props.onFileOpen).toHaveBeenCalledWith(docs[0]);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select millet.md' }));
    expect(h.store.toggleSelection).toHaveBeenCalledWith('millet');
    fireEvent.contextMenu(row('rye.md'));
    expect(props.onContextMenu).toHaveBeenCalledWith(expect.anything(), docs[0]);
    fireEvent.click(screen.getAllByRole('button', { name: 'Rename' })[0]);
    fireEvent.click(screen.getAllByRole('button', { name: 'Delete' })[0]);
    expect(props.onRename).toHaveBeenCalledWith(docs[0]);
    expect(props.onDelete).toHaveBeenCalledWith(docs[0]);
  });

  it('works without handlers', () => {
    render(<ListView />);
    fireEvent.doubleClick(row('rye.md'));
    fireEvent.contextMenu(row('rye.md'));
    expect(screen.queryByRole('button', { name: 'Rename' })).not.toBeInTheDocument();
  });

  it('shows loading, errors and empty states', () => {
    h.query = { ...h.query, isLoading: true };
    const loading = render(<ListView />);
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
    loading.unmount();
    h.query = { documents: docs, filteredDocuments: docs, isLoading: false, error: 'index unreadable' };
    const failing = render(<ListView />);
    expect(screen.getByText('index unreadable')).toBeInTheDocument();
    failing.unmount();
    h.query = { documents: [], filteredDocuments: [], isLoading: false, error: null };
    const onAddFolder = vi.fn();
    const empty = render(<ListView onAddFolder={onAddFolder} />);
    fireEvent.click(screen.getByRole('button', { name: 'Add a folder' }));
    expect(onAddFolder).toHaveBeenCalled();
    empty.unmount();
    const noHandler = render(<ListView />);
    fireEvent.click(screen.getByRole('button', { name: 'Add a folder' }));
    noHandler.unmount();
    h.query = { documents: docs, filteredDocuments: [], isLoading: false, error: null };
    render(<ListView />);
    expect(screen.getByText('No files match.')).toBeInTheDocument();
  });
});
