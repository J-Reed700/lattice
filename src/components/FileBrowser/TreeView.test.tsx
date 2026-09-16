import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TreeView } from './TreeView';
import { useLibraryDocumentsQuery } from '../../hooks/queries/useLibraryDocumentsQuery';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';

import type { DocumentMetadata } from '../../types/fileBrowser';

vi.mock('../../hooks/queries/useLibraryDocumentsQuery', () => ({
  useLibraryDocumentsQuery: vi.fn(),
}));

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 56,
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({
      index,
      start: index * 56,
      key: index,
    })),
  }),
}));

const document = (id: string, filePath: string, fileName = 'Research.pdf'): DocumentMetadata => ({
  id,
  filePath,
  fileName,
  fileType: 'pdf',
  category: 'document',
  language: 'en',
  modifiedAt: '2026-09-15T00:00:00Z',
  indexedAt: '2026-09-15T00:00:00Z',
  wordCount: 100,
});

const setDocuments = (documents: DocumentMetadata[], filteredDocuments = documents) => {
  vi.mocked(useLibraryDocumentsQuery).mockReturnValue({
    documents,
    filteredDocuments,
    isLoading: false,
    error: null,
    refreshFiles: vi.fn(),
  });
};

describe('TreeView storage groups', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useFileBrowserStore.setState({ selectedDocumentIds: new Set(), focusedDocumentId: null });
  });

  it('shows imported filenames directly, preserving their order, identity, and opening paths', () => {
    const hash = 'a'.repeat(64);
    const documents = [
      document('first', `/Users/test/.lattice/files/${hash}/Research.pdf`),
      document('second', `C:\\Users\\test\\.lattice\\files\\${'b'.repeat(64)}\\Research.pdf`),
    ];
    setDocuments(documents);
    const onFileOpen = vi.fn();
    const { container, rerender } = render(<TreeView onFileOpen={onFileOpen} />);

    const group = screen.getByRole('button', { name: /^Imported files\s*2$/ });
    expect(group).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getAllByText('Research.pdf')).toHaveLength(2);
    expect(container).not.toHaveTextContent(hash);
    expect(container).not.toHaveTextContent('.lattice');

    const rows = screen.getAllByRole('button', { name: /^Select Research\.pdf/ });
    fireEvent.click(rows[1]);
    expect(useFileBrowserStore.getState().selectedDocumentIds).toEqual(new Set(['second']));
    expect(useFileBrowserStore.getState().focusedDocumentId).toBe('second');
    fireEvent.doubleClick(rows[0]);
    fireEvent.doubleClick(rows[1]);
    expect(onFileOpen.mock.calls).toEqual([[documents[0]], [documents[1]]]);

    fireEvent.click(group);
    expect(screen.queryByText('Research.pdf')).not.toBeInTheDocument();
    fireEvent.click(group);
    expect(screen.getAllByText('Research.pdf')).toHaveLength(2);

    setDocuments(documents, [documents[1]]);
    rerender(<TreeView onFileOpen={onFileOpen} />);
    expect(screen.getByRole('button', { name: /^Imported files\s*1$/ })).toBeInTheDocument();
    expect(screen.getAllByText('Research.pdf')).toHaveLength(1);
  });

  it('keeps real folders, including hash names and folders named Imported files, separate', () => {
    const hash = 'c'.repeat(64);
    setDocuments([
      document('import', `/Users/test/.lattice/files/${hash}/Import.pdf`, 'Import.pdf'),
      document('local', `Projects/${hash}/Local.pdf`, 'Local.pdf'),
      document('named-folder', 'Imported files/Own.pdf', 'Own.pdf'),
      document('other-storage', `Projects/files/${hash}/Other.pdf`, 'Other.pdf'),
    ]);
    render(<TreeView />);

    expect(screen.getByText(hash)).toBeInTheDocument();
    expect(screen.getByText(`files/${hash}`)).toBeInTheDocument();
    const groups = screen.getAllByRole('button', { name: /^Imported files\s*1$/ });
    expect(groups).toHaveLength(2);
    fireEvent.click(groups[0]);
    expect(screen.queryByText('Import.pdf')).not.toBeInTheDocument();
    expect(screen.getByText('Own.pdf')).toBeInTheDocument();
    expect(screen.getByText('Local.pdf')).toBeInTheDocument();
    expect(screen.getByText('Other.pdf')).toBeInTheDocument();
  });

  it('groups URLs and archived web pages under Web without exposing archive directories', () => {
    setDocuments([
      document('url', 'https://example.com/article', 'Live article'),
      document('archive', '/Users/test/.lattice/web-archive/2026/09/15/article/index.html', 'Saved article'),
    ]);
    const { container } = render(<TreeView />);

    const group = screen.getByRole('button', { name: /^Web\s*2$/ });
    expect(within(group).getByText('2')).toBeInTheDocument();
    expect(screen.getByText('Live article')).toBeInTheDocument();
    expect(screen.getByText('Saved article')).toBeInTheDocument();
    expect(container).not.toHaveTextContent('web-archive');
    fireEvent.click(group);
    expect(screen.queryByText('Saved article')).not.toBeInTheDocument();
    expect(screen.queryByText('Live article')).not.toBeInTheDocument();
  });
});
