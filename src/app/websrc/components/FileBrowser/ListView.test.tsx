import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { ListView } from './ListView';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';

import type { DocumentMetadata, FileNode } from '../../types/fileBrowser';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 50,
    getVirtualItems: () =>
      Array.from({ length: count }, (_value, index) => ({
        index,
        start: index * 50,
        size: 50,
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
  selectDensity: (state: any) => state.density || 'comfortable',
}));

describe('ListView', () => {
  const mockFiles: FileNode[] = [
    {
      id: '1',
      name: 'document.pdf',
      path: '/path/document.pdf',
      type: 'file',
      size: 1024000,
      modified: '2024-01-01',
      isIndexed: true,
    },
    {
      id: '2',
      name: 'image.png',
      path: '/path/image.png',
      type: 'file',
      size: 512000,
      modified: '2024-01-02',
      isIndexed: true,
    },
  ];
  const mockDocuments: DocumentMetadata[] = mockFiles.map(file => ({
    id: file.id,
    fileName: file.name,
    filePath: file.path,
    fileType: file.name.split('.').pop() || 'txt',
    category: 'document',
    language: 'en',
    modifiedAt: file.modified,
    indexedAt: file.modified,
    wordCount: Math.max(1, Math.floor(file.size / 100)),
  }));

  const mockSetSortField = vi.fn();
  const mockToggleSortOrder = vi.fn();
  const mockSelectFile = vi.fn();
  const mockToggleSelection = vi.fn();
  const mockClearSelection = vi.fn();
  const mockSelectAll = vi.fn();
  const mockLoadFiles = vi.fn();

  const mockSetViewMode = vi.fn();
  const mockSetCurrentPath = vi.fn();
  const mockNavigateUp = vi.fn();
  const mockNavigateTo = vi.fn();
  const mockDeselectFile = vi.fn();
  const mockSelectRange = vi.fn();
  const mockToggleFolder = vi.fn();
  const mockExpandFolder = vi.fn();
  const mockCollapseFolder = vi.fn();
  const mockExpandAll = vi.fn();
  const mockCollapseAll = vi.fn();
  const mockSetSortOrder = vi.fn();
  const mockSetSearchQuery = vi.fn();
  const mockSetFilterByType = vi.fn();
  const mockRefreshFiles = vi.fn();
  const mockOpenContextMenu = vi.fn();
  const mockCloseContextMenu = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        // View configuration
        viewMode: 'list' as const,
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
        files: mockFiles,
        documents: mockDocuments,
        fileTree: [],
        isLoading: false,
        error: null,

        // Context menu
        contextMenuPosition: null,
        contextMenuFile: null,

        // Actions
        setViewMode: mockSetViewMode,
        setCurrentPath: mockSetCurrentPath,
        navigateUp: mockNavigateUp,
        navigateTo: mockNavigateTo,
        selectFile: mockSelectFile,
        deselectFile: mockDeselectFile,
        toggleSelection: mockToggleSelection,
        selectAll: mockSelectAll,
        clearSelection: mockClearSelection,
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
  });

  it('renders file list', () => {
    render(<ListView />);

    expect(screen.getByText('document.pdf')).toBeInTheDocument();
    expect(screen.getByText('image.png')).toBeInTheDocument();
  });

  it('shows loading state', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        viewMode: 'list' as const,
        currentPath: '',
        selectedDocumentIds: new Set<string>(),
        expandedFolders: new Set<string>(),
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,
        files: [],
        documents: [],
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

    render(<ListView />);
    expect(screen.getByRole('status', { name: /loading file list/i })).toBeInTheDocument();
  });

  it('shows error state', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        viewMode: 'list' as const,
        currentPath: '',
        selectedDocumentIds: new Set<string>(),
        expandedFolders: new Set<string>(),
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,
        files: [],
        documents: [],
        fileTree: [],
        isLoading: false,
        error: 'Failed to load files',
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

    render(<ListView />);
    expect(screen.getByText('Error loading files')).toBeInTheDocument();
    expect(screen.getByText('Failed to load files')).toBeInTheDocument();
  });

  it('shows empty state', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        viewMode: 'list' as const,
        currentPath: '',
        selectedDocumentIds: new Set<string>(),
        expandedFolders: new Set<string>(),
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,
        files: [],
        documents: [],
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

    render(<ListView />);
    expect(screen.getByText('No documents found')).toBeInTheDocument();
  });

  describe('Sorting', () => {
    it('toggles sort order when clicking same column header', async () => {
      const user = userEvent.setup();
      render(<ListView />);

      const nameHeader = screen.getByText('Name');
      await user.click(nameHeader);

      expect(mockToggleSortOrder).toHaveBeenCalled();
    });

    it('changes sort field when clicking different column header', async () => {
      const user = userEvent.setup();
      render(<ListView />);

      const sizeHeader = screen.getByText('Words');
      await user.click(sizeHeader);

      expect(mockSetSortField).toHaveBeenCalledWith('size');
    });
  });

  describe('Selection', () => {
    it('selects file on single click', async () => {
      const user = userEvent.setup();
      render(<ListView />);

      const file = screen.getByText('document.pdf');
      await user.click(file);

      expect(mockClearSelection).toHaveBeenCalled();
      expect(mockSelectFile).toHaveBeenCalledWith('1');
    });

    it('toggles selection with Ctrl+click', async () => {
      render(<ListView />);

      const file = screen.getByText('document.pdf');
      fireEvent.click(file, { ctrlKey: true });

      expect(mockToggleSelection).toHaveBeenCalledWith('1');
    });

    it('toggles selection with Meta+click', async () => {
      render(<ListView />);

      const file = screen.getByText('document.pdf');
      fireEvent.click(file, { metaKey: true });

      expect(mockToggleSelection).toHaveBeenCalledWith('1');
    });

    it('toggles selection via checkbox', async () => {
      const user = userEvent.setup();
      render(<ListView />);

      const checkboxes = screen.getAllByRole('checkbox');
      await user.click(checkboxes[1]);

      expect(mockToggleSelection).toHaveBeenCalledWith('1');
    });

    it('selects all files when header checkbox clicked', async () => {
      const user = userEvent.setup();
      render(<ListView />);

      const checkboxes = screen.getAllByRole('checkbox');
      await user.click(checkboxes[0]);

      expect(mockSelectAll).toHaveBeenCalled();
    });
  });

  describe('File opening', () => {
    it('opens file on double click', async () => {
      const mockOnFileOpen = vi.fn();
      const user = userEvent.setup();
      render(<ListView onFileOpen={mockOnFileOpen} />);

      const file = screen.getByText('document.pdf');
      await user.dblClick(file);

      expect(mockOnFileOpen).toHaveBeenCalledWith(mockDocuments[0]);
    });
  });

  describe('Context menu', () => {
    it('triggers context menu on right click', async () => {
      const mockOnContextMenu = vi.fn();
      render(<ListView onContextMenu={mockOnContextMenu} />);

      const file = screen.getByText('document.pdf');
      fireEvent.contextMenu(file);

      expect(mockOnContextMenu).toHaveBeenCalled();
    });
  });

  it('displays selected files with highlight', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    vi.mocked(useFileBrowserStore).mockImplementation((selector: any) => {
      const state = {
        viewMode: 'list' as const,
        currentPath: '',
        selectedDocumentIds: new Set(['1']),
        expandedFolders: new Set<string>(),
        sortField: 'name' as const,
        sortOrder: 'asc' as const,
        searchQuery: '',
        filterByType: null,
        files: mockFiles,
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

    render(<ListView />);

    const checkbox = screen.getByLabelText('Select document.pdf');
    expect(checkbox).toBeChecked();
  });
});
