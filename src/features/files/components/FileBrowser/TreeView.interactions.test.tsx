import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { TreeView } from './TreeView';

const h = vi.hoisted(() => ({ query: {} as Record<string, unknown>, store: {} as Record<string, any> }));

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

const doc = (id: string, filePath: string, overrides: Partial<DocumentMetadata> = {}) => ({
  id, fileName: filePath.split('/').pop() || id, filePath, fileType: 'md', category: 'document', modifiedAt: '', indexedAt: '', ...overrides,
}) as DocumentMetadata;

const docs = [
  doc('bread', '/kitchen/breads/sourdough.md'),
  doc('loose', 'loose-note.md'),
  doc('bare', '/'),
];

const row = (name: string) => screen.getByRole('checkbox', { name: `Select ${name}` }).closest('[aria-selected]') as HTMLElement;

describe('TreeView interactions', () => {
  beforeEach(() => {
    h.query = { documents: docs, filteredDocuments: docs, isLoading: false, error: null };
    h.store = {
      selectedDocumentIds: new Set<string>(),
      selectFile: vi.fn(),
      toggleSelection: vi.fn(),
      clearSelection: vi.fn(),
      setFocusedDocument: vi.fn(),
      searchQuery: '',
      setSearchQuery: vi.fn(),
    };
  });

  it('puts documents with no folder under Ungrouped, and collapses a folder', () => {
    render(<TreeView />);
    const ungrouped = screen.getByRole('button', { name: /Ungrouped/ });
    expect(ungrouped).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('loose-note.md')).toBeInTheDocument();
    fireEvent.click(ungrouped);
    expect(screen.getByRole('button', { name: /Ungrouped/ })).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText('loose-note.md')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Ungrouped/ }));
    expect(screen.getByText('loose-note.md')).toBeInTheDocument();
  });

  it('selects on click and toggles with a modifier', () => {
    render(<TreeView />);
    fireEvent.click(row('sourdough.md'));
    expect(h.store.clearSelection).toHaveBeenCalled();
    expect(h.store.selectFile).toHaveBeenCalledWith('bread');
    expect(h.store.setFocusedDocument).toHaveBeenCalledWith('bread');
    fireEvent.click(row('loose-note.md'), { metaKey: true });
    expect(h.store.toggleSelection).toHaveBeenCalledWith('loose');
    expect(h.store.selectFile).toHaveBeenCalledTimes(1);
  });

  it('opens, toggles, renames, deletes and shows the menu for a document', () => {
    const props = { onFileOpen: vi.fn(), onContextMenu: vi.fn(), onRename: vi.fn(), onDelete: vi.fn() };
    h.store.selectedDocumentIds = new Set(['bread']);
    render(<TreeView {...props} />);
    fireEvent.doubleClick(row('sourdough.md'));
    fireEvent.contextMenu(row('sourdough.md'));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select loose-note.md' }));
    fireEvent.click(screen.getAllByRole('button', { name: 'Rename' })[0]);
    fireEvent.click(screen.getAllByRole('button', { name: 'Delete' })[0]);
    expect(props.onFileOpen).toHaveBeenCalledWith(docs[0]);
    expect(props.onContextMenu).toHaveBeenCalledWith(expect.anything(), docs[0]);
    expect(h.store.toggleSelection).toHaveBeenCalledWith('loose');
    expect(props.onRename).toHaveBeenCalledWith(docs[0]);
    expect(props.onDelete).toHaveBeenCalledWith(docs[0]);
  });

  it('works without handlers', () => {
    render(<TreeView />);
    fireEvent.doubleClick(row('sourdough.md'));
    fireEvent.contextMenu(row('sourdough.md'));
    expect(screen.queryByRole('button', { name: 'Rename' })).not.toBeInTheDocument();
  });

  it('shows loading, errors and empty states', () => {
    h.query = { ...h.query, isLoading: true };
    const loading = render(<TreeView />);
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
    loading.unmount();
    h.query = { documents: docs, filteredDocuments: docs, isLoading: false, error: 'tree unreadable' };
    const failing = render(<TreeView />);
    expect(screen.getByText('tree unreadable')).toBeInTheDocument();
    failing.unmount();
    h.query = { documents: [], filteredDocuments: [], isLoading: false, error: null };
    const onAddFolder = vi.fn();
    const empty = render(<TreeView onAddFolder={onAddFolder} />);
    fireEvent.click(screen.getByRole('button', { name: 'Add a folder' }));
    expect(onAddFolder).toHaveBeenCalled();
    empty.unmount();
    const noHandler = render(<TreeView />);
    fireEvent.click(screen.getByRole('button', { name: 'Add a folder' }));
    noHandler.unmount();
    h.query = { documents: docs, filteredDocuments: [], isLoading: false, error: null };
    render(<TreeView />);
    expect(screen.getByText('No files match.')).toBeInTheDocument();
  });
});
