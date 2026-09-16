import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { GridView } from './GridView';
import { useLibraryDocumentsQuery } from '../../hooks/queries/useLibraryDocumentsQuery';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';

import type { DocumentMetadata } from '../../types/fileBrowser';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 280,
    getVirtualItems: () =>
      Array.from({ length: count }, (_value, index) => ({
        index,
        start: index * 280,
        size: 280,
        key: `row-${index}`,
      })),
  }),
}));

vi.mock('../../stores/fileBrowserStore', () => ({
  useFileBrowserStore: vi.fn(),
  selectFilteredDocuments: (state: any) => {
    const { documents, searchQuery } = state;
    if (!searchQuery?.trim()) return documents || [];
    const query = searchQuery.toLowerCase();
    return (documents || []).filter((doc: any) =>
      doc.fileName.toLowerCase().includes(query)
    );
  },
}));

vi.mock('../../hooks/queries/useLibraryDocumentsQuery', () => ({
  useLibraryDocumentsQuery: vi.fn(),
}));

describe('GridView', () => {
  const mockDocuments: DocumentMetadata[] = [
    {
      id: '1',
      fileName: 'document.pdf',
      filePath: '/path/document.pdf',
      fileType: 'pdf',
      category: 'document',
      language: 'en',
      modifiedAt: '2024-01-01',
      indexedAt: '2024-01-01',
      wordCount: 1024,
    },
    {
      id: '2',
      fileName: 'image.png',
      filePath: '/path/image.png',
      fileType: 'png',
      category: 'image',
      language: 'en',
      modifiedAt: '2024-01-02',
      indexedAt: '2024-01-02',
      wordCount: 256,
    },
    {
      id: '3',
      fileName: 'folder',
      filePath: '/path/folder',
      fileType: 'folder',
      category: 'document',
      language: 'en',
      modifiedAt: '2024-01-03',
      indexedAt: '2024-01-03',
      wordCount: 0,
    },
  ];

  const mockSelectFile = vi.fn();
  const mockToggleSelection = vi.fn();
  const mockClearSelection = vi.fn();
  const mockLoadFiles = vi.fn();

  const mockSetViewMode = vi.fn();
  const mockSetCurrentPath = vi.fn();
  const mockNavigateUp = vi.fn();
  const mockNavigateTo = vi.fn();
  const mockDeselectFile = vi.fn();
  const mockSetFocusedDocument = vi.fn();
  const mockSelectRange = vi.fn();
  const mockToggleFolder = vi.fn();
  const mockExpandFolder = vi.fn();
  const mockCollapseFolder = vi.fn();
  const mockExpandAll = vi.fn();
  const mockCollapseAll = vi.fn();
  const mockSetSortField = vi.fn();
  const mockSetSortOrder = vi.fn();
  const mockToggleSortOrder = vi.fn();
  const mockSetSearchQuery = vi.fn();
  const mockSetFilterByType = vi.fn();
  const mockRefreshFiles = vi.fn();
  const mockOpenContextMenu = vi.fn();
  const mockCloseContextMenu = vi.fn();
  const mockSelectAll = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        // View configuration
        viewMode: 'grid' as const,
        currentPath: '',
        selectedDocumentIds: new Set<string>(),
        expandedFolders: new Set<string>(),
        groupByDate: false,
        density: 'comfortable' as const,

        // Sorting and filtering
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,

        // Data
        files: [],
        documents: mockDocuments,
        fileTree: [],
        isLoading: false,
        error: null,

        // Context menu
        contextMenuPosition: null,
        contextMenuFile: null,

        setViewMode: mockSetViewMode,
        setCurrentPath: mockSetCurrentPath,
        navigateUp: mockNavigateUp,
        navigateTo: mockNavigateTo,
        selectFile: mockSelectFile,
        deselectFile: mockDeselectFile,
        toggleSelection: mockToggleSelection,
        selectAll: mockSelectAll,
        clearSelection: mockClearSelection,
        setFocusedDocument: mockSetFocusedDocument,
        selectRange: mockSelectRange,
        toggleFolder: mockToggleFolder,
        expandFolder: mockExpandFolder,
        collapseFolder: mockCollapseFolder,
        expandAll: mockExpandAll,
        collapseAll: mockCollapseAll,
        setSortField: mockSetSortField,
        setSortOrder: mockSetSortOrder,
        toggleSortOrder: mockToggleSortOrder,
        setSearchQuery: mockSetSearchQuery,
        setFilterByType: mockSetFilterByType,
        loadFiles: mockLoadFiles,
        refreshFiles: mockRefreshFiles,
        openContextMenu: mockOpenContextMenu,
        closeContextMenu: mockCloseContextMenu,
      };

      // If selector is a function, call it with state
      // Otherwise return the whole state
      if (typeof selector === 'function') {
        return selector(state);
      }
      return state;
    });
    vi.mocked(useLibraryDocumentsQuery).mockImplementation(() => {
      const documents = useFileBrowserStore((state: any) => state.documents) ?? [];
      return {
        documents,
        filteredDocuments: documents,
        isLoading: useFileBrowserStore((state: any) => state.isLoading) ?? false,
        error: useFileBrowserStore((state: any) => state.error) ?? null,
        refreshFiles: vi.fn(),
      };
    });
  });

  it('renders file grid', () => {
    render(<GridView />);

    expect(screen.getByText('document.pdf')).toBeInTheDocument();
    expect(screen.getByText('image.png')).toBeInTheDocument();
    expect(screen.getByText('folder')).toBeInTheDocument();
  });

  it('does not trigger file loading on mount', () => {
    render(<GridView />);
    expect(mockLoadFiles).not.toHaveBeenCalled();
  });

  it('shows loading state', () => {
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        viewMode: 'grid' as const,
        currentPath: '',
        selectedDocumentIds: new Set<string>(),
        expandedFolders: new Set<string>(),
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,
        files: [],
        fileTree: [],
        isLoading: true,
        error: null,
        contextMenuPosition: null,
        contextMenuFile: null,
        setViewMode: mockSetViewMode,
        setCurrentPath: mockSetCurrentPath,
        navigateUp: mockNavigateUp,
        navigateTo: mockNavigateTo,
        selectFile: mockSelectFile,
        deselectFile: mockDeselectFile,
        toggleSelection: mockToggleSelection,
        selectAll: mockSelectAll,
        clearSelection: mockClearSelection,
        setFocusedDocument: mockSetFocusedDocument,
        selectRange: mockSelectRange,
        toggleFolder: mockToggleFolder,
        expandFolder: mockExpandFolder,
        collapseFolder: mockCollapseFolder,
        expandAll: mockExpandAll,
        collapseAll: mockCollapseAll,
        setSortField: mockSetSortField,
        setSortOrder: mockSetSortOrder,
        toggleSortOrder: mockToggleSortOrder,
        setSearchQuery: mockSetSearchQuery,
        setFilterByType: mockSetFilterByType,
        loadFiles: mockLoadFiles,
        refreshFiles: mockRefreshFiles,
        openContextMenu: mockOpenContextMenu,
        closeContextMenu: mockCloseContextMenu,
      };
      return selector(state);
    });

    render(<GridView />);
    expect(screen.getByRole('status', { name: /loading file grid/i })).toBeInTheDocument();
  });

  it('shows error state', () => {
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        viewMode: 'grid' as const,
        currentPath: '',
        selectedDocumentIds: new Set<string>(),
        expandedFolders: new Set<string>(),
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,
        files: [],
        fileTree: [],
        isLoading: false,
        error: 'Connection failed',
        contextMenuPosition: null,
        contextMenuFile: null,
        setViewMode: mockSetViewMode,
        setCurrentPath: mockSetCurrentPath,
        navigateUp: mockNavigateUp,
        navigateTo: mockNavigateTo,
        selectFile: mockSelectFile,
        deselectFile: mockDeselectFile,
        toggleSelection: mockToggleSelection,
        selectAll: mockSelectAll,
        clearSelection: mockClearSelection,
        setFocusedDocument: mockSetFocusedDocument,
        selectRange: mockSelectRange,
        toggleFolder: mockToggleFolder,
        expandFolder: mockExpandFolder,
        collapseFolder: mockCollapseFolder,
        expandAll: mockExpandAll,
        collapseAll: mockCollapseAll,
        setSortField: mockSetSortField,
        setSortOrder: mockSetSortOrder,
        toggleSortOrder: mockToggleSortOrder,
        setSearchQuery: mockSetSearchQuery,
        setFilterByType: mockSetFilterByType,
        loadFiles: mockLoadFiles,
        refreshFiles: mockRefreshFiles,
        openContextMenu: mockOpenContextMenu,
        closeContextMenu: mockCloseContextMenu,
      };
      return selector(state);
    });

    render(<GridView />);
    expect(screen.getByText("Couldn’t load your files.")).toBeInTheDocument();
    expect(screen.getByText('Connection failed')).toBeInTheDocument();
  });

  it('shows empty state', () => {
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        viewMode: 'grid' as const,
        currentPath: '',
        selectedDocumentIds: new Set<string>(),
        expandedFolders: new Set<string>(),
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,
        files: [],
        fileTree: [],
        isLoading: false,
        error: null,
        contextMenuPosition: null,
        contextMenuFile: null,
        setViewMode: mockSetViewMode,
        setCurrentPath: mockSetCurrentPath,
        navigateUp: mockNavigateUp,
        navigateTo: mockNavigateTo,
        selectFile: mockSelectFile,
        deselectFile: mockDeselectFile,
        toggleSelection: mockToggleSelection,
        selectAll: mockSelectAll,
        clearSelection: mockClearSelection,
        setFocusedDocument: mockSetFocusedDocument,
        selectRange: mockSelectRange,
        toggleFolder: mockToggleFolder,
        expandFolder: mockExpandFolder,
        collapseFolder: mockCollapseFolder,
        expandAll: mockExpandAll,
        collapseAll: mockCollapseAll,
        setSortField: mockSetSortField,
        setSortOrder: mockSetSortOrder,
        toggleSortOrder: mockToggleSortOrder,
        setSearchQuery: mockSetSearchQuery,
        setFilterByType: mockSetFilterByType,
        loadFiles: mockLoadFiles,
        refreshFiles: mockRefreshFiles,
        openContextMenu: mockOpenContextMenu,
        closeContextMenu: mockCloseContextMenu,
      };
      return selector(state);
    });

    render(<GridView />);
    expect(screen.getByText('Nothing indexed yet.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add a folder' })).toBeInTheDocument();
  });

  describe('Selection', () => {
    it('selects file on single click', async () => {
      const user = userEvent.setup();
      render(<GridView />);

      const file = screen.getByText('document.pdf');
      await user.click(file);

      expect(mockClearSelection).toHaveBeenCalled();
      expect(mockSelectFile).toHaveBeenCalledWith('1');
    });

    it('toggles selection with Ctrl+click', async () => {
      render(<GridView />);

      const file = screen.getByText('document.pdf');
      fireEvent.click(file, { ctrlKey: true });

      expect(mockToggleSelection).toHaveBeenCalledWith('1');
    });

    it('toggles selection with Meta+click', async () => {
      render(<GridView />);

      const file = screen.getByText('document.pdf');
      fireEvent.click(file, { metaKey: true });

      expect(mockToggleSelection).toHaveBeenCalledWith('1');
    });
  });

  describe('File opening', () => {
    it('opens file on double click', async () => {
      const mockOnFileOpen = vi.fn();
      const user = userEvent.setup();
      render(<GridView onFileOpen={mockOnFileOpen} />);

      const file = screen.getByText('document.pdf');
      await user.dblClick(file);

      expect(mockOnFileOpen).toHaveBeenCalledWith(mockDocuments[0]);
    });
  });

  describe('Context menu', () => {
    it('triggers context menu on right click', async () => {
      const mockOnContextMenu = vi.fn();
      const user = userEvent.setup();
      render(<GridView onContextMenu={mockOnContextMenu} />);

      const file = screen.getByText('document.pdf');
      await user.pointer({ keys: '[MouseRight>]', target: file });

      expect(mockOnContextMenu).toHaveBeenCalled();
    });
  });

  it('displays selected files with highlight', () => {
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        viewMode: 'grid' as const,
        currentPath: '',
        selectedDocumentIds: new Set(['1']),
        expandedFolders: new Set<string>(),
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,
        files: [],
        documents: mockDocuments,
        fileTree: [],
        isLoading: false,
        error: null,
        contextMenuPosition: null,
        contextMenuFile: null,
        setViewMode: mockSetViewMode,
        setCurrentPath: mockSetCurrentPath,
        navigateUp: mockNavigateUp,
        navigateTo: mockNavigateTo,
        selectFile: mockSelectFile,
        deselectFile: mockDeselectFile,
        toggleSelection: mockToggleSelection,
        selectAll: mockSelectAll,
        clearSelection: mockClearSelection,
        setFocusedDocument: mockSetFocusedDocument,
        selectRange: mockSelectRange,
        toggleFolder: mockToggleFolder,
        expandFolder: mockExpandFolder,
        collapseFolder: mockCollapseFolder,
        expandAll: mockExpandAll,
        collapseAll: mockCollapseAll,
        setSortField: mockSetSortField,
        setSortOrder: mockSetSortOrder,
        toggleSortOrder: mockToggleSortOrder,
        setSearchQuery: mockSetSearchQuery,
        setFilterByType: mockSetFilterByType,
        loadFiles: mockLoadFiles,
        refreshFiles: mockRefreshFiles,
        openContextMenu: mockOpenContextMenu,
        closeContextMenu: mockCloseContextMenu,
      };
      return selector(state);
    });

    render(<GridView />);

    const selectedCard = screen.getByText('document.pdf').closest('.group');
    expect(selectedCard).toHaveClass('border-border-default');
    expect(selectedCard).toHaveClass('bg-surface-raised');
  });
});
