import type { ReactNode } from 'react';

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { FileBrowser } from './FileBrowser';

type Props = Record<string, any>;

const h = vi.hoisted(() => ({
  props: {} as Record<string, Props>,
  query: {} as Record<string, any>,
  store: {} as Record<string, any>,
  navigate: vi.fn(),
  openDocumentFile: vi.fn(),
  resolveDocumentPath: vi.fn(),
  documentActions: { deleteDocuments: { mutateAsync: vi.fn(), isPending: false } },
  deletion: {} as Record<string, any>,
  commands: {} as Record<string, any>,
  collections: {} as Record<string, any>,
  themes: {} as Record<string, any>,
  readout: {} as Record<string, any>,
  savedSearchDialog: {} as Record<string, any>,
  paletteArgs: {} as Props,
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));

const { stub } = vi.hoisted(() => ({
  stub: (name: string, render?: (props: Props) => ReactNode) => (props: Props) => {
    h.props[name] = props;
    return render ? <>{render(props)}</> : <div data-testid={name} />;
  },
}));

vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/features/files/hooks/useLibraryDocumentsQuery', () => ({ useLibraryDocumentsQuery: () => h.query }));
vi.mock('@/features/files/stores/fileBrowserStore', () => ({
  useFileBrowserStore: (selector: (state: unknown) => unknown) => selector(h.store),
}));
vi.mock('@/features/files/hooks/useLibraryDocumentActions', () => ({
  openDocumentFile: h.openDocumentFile,
  resolveDocumentPath: h.resolveDocumentPath,
  useLibraryDocumentActions: () => h.documentActions,
}));
vi.mock('./useLibraryDeletion', () => ({ useLibraryDeletion: () => h.deletion }));
vi.mock('./useLibraryDocumentCommands', () => ({ useLibraryDocumentCommands: () => h.commands }));
vi.mock('./useLibraryCollections', () => ({ useLibraryCollections: () => h.collections }));
vi.mock('./useLibraryThemes', () => ({ useLibraryThemes: () => h.themes }));
vi.mock('./useLibraryReadout', () => ({ useLibraryReadout: () => h.readout }));
vi.mock('./useLibraryContentSearch', () => ({ useLibraryContentSearch: () => ({ activeFilter: h.store.activeFilter ?? null }) }));
vi.mock('./useLibraryPaletteCommands', () => ({ useLibraryPaletteCommands: (args: Props) => { h.paletteArgs = args; } }));
vi.mock('./useSavedSearchNameDialog', () => ({ useSavedSearchNameDialog: () => h.savedSearchDialog }));
vi.mock('./LibraryViewActions', () => ({
  LibraryViewActions: stub('LibraryViewActions', (p) => <button type="button" onClick={() => void p.handleRefresh()}>Refresh</button>),
}));
vi.mock('./LibraryToolbar', () => ({
  LibraryToolbar: stub('LibraryToolbar', (p) => <button type="button" onClick={p.onToggleRail}>Toggle rail</button>),
}));
vi.mock('./LibraryRail', () => ({ LibraryRail: stub('LibraryRail') }));
vi.mock('./TypeFacets', () => ({ TypeFacets: stub('TypeFacets') }));
vi.mock('./SelectionBar', () => ({ SelectionBar: stub('SelectionBar') }));
vi.mock('./TreeView', () => ({ TreeView: stub('TreeView') }));
vi.mock('./ListView', () => ({ ListView: stub('ListView') }));
vi.mock('./GridView', () => ({ GridView: stub('GridView') }));
vi.mock('./ContextMenu', () => ({ ContextMenu: stub('ContextMenu') }));
vi.mock('./RenameDialog', () => ({
  RenameDialog: stub('RenameDialog', (p) => (
    <div role="dialog" aria-label="Rename">
      <button type="button" onClick={p.onSuccess}>Renamed</button>
      <button type="button" onClick={p.onClose}>Close rename</button>
    </div>
  )),
}));
vi.mock('./SavedSearchNameDialog', () => ({ SavedSearchNameDialog: stub('SavedSearchNameDialog') }));
vi.mock('./LibraryCollectionDialogs', () => ({ LibraryCollectionDialogs: stub('LibraryCollectionDialogs') }));
vi.mock('./LibraryDeleteDialogs', () => ({ LibraryDeleteDialogs: stub('LibraryDeleteDialogs') }));
vi.mock('../Neighborhood', () => ({ NeighborhoodPanel: stub('NeighborhoodPanel') }));
vi.mock('@/features/reading/components/FilePreviewModal', () => ({ FilePreviewModal: stub('FilePreviewModal') }));

const doc = (id: string, overrides: Partial<DocumentMetadata> = {}) => ({
  id, fileName: `${id}.md`, filePath: `/library/${id}.md`, fileType: 'md', category: 'document',
  modifiedAt: '2026-01-01', indexedAt: '2026-01-01', ...overrides,
}) as DocumentMetadata;

const docs = [doc('herbs'), doc('spices')];

const makeStore = (overrides: Record<string, unknown> = {}) => ({
  viewMode: 'list',
  setViewMode: vi.fn(),
  selectedDocumentIds: new Set<string>(),
  selectAll: vi.fn(),
  clearSelection: vi.fn(),
  scope: { kind: 'all' },
  setScope: vi.fn(),
  sourceConnections: [],
  searchQuery: '',
  setSearchQuery: vi.fn(),
  filterByType: null,
  setFilterByType: vi.fn(),
  filterBySource: 'all',
  setFilterBySource: vi.fn(),
  sortField: 'name',
  sortOrder: 'asc',
  setSortField: vi.fn(),
  setSortOrder: vi.fn(),
  groupByDate: false,
  toggleGroupByDate: vi.fn(),
  savedSearches: [],
  activeSavedSearchId: null,
  createSavedSearch: vi.fn(() => 'saved-1'),
  applySavedSearch: vi.fn(),
  isContentSearchLoading: false,
  contextMenuPosition: null,
  contextMenuDocument: null,
  openContextMenu: vi.fn(),
  closeContextMenu: vi.fn(),
  focusedDocumentId: null,
  setFocusedDocument: vi.fn(),
  isNeighborhoodOpen: false,
  toggleNeighborhood: vi.fn(),
  ...overrides,
});

const preview = () => h.props.FilePreviewModal;

describe('FileBrowser', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.props = {};
    h.query = {
      documents: docs,
      filteredDocuments: docs,
      refreshFiles: vi.fn().mockResolvedValue(undefined),
      collections: [],
      collectionsError: null,
      refreshCollections: vi.fn().mockResolvedValue(undefined),
    };
    h.store = makeStore();
    h.deletion = { handleDelete: vi.fn(), setPendingBulkDelete: vi.fn() };
    h.commands = {
      spaces: [], isLoadingSpaces: false, isAssigningSpace: false,
      reindexDocument: vi.fn(), removeFromIndex: vi.fn(), handleBulkAssignToSpace: vi.fn(),
    };
    h.collections = {
      activeCollection: null,
      collectionCounts: new Map(),
      setCollectionSelection: vi.fn(),
      setAddDocumentsCollectionId: vi.fn(),
      setRenameCollection: vi.fn(),
      setPendingDeleteCollection: vi.fn(),
      handleCreateCollection: vi.fn(),
      handleRemoveFromCollection: vi.fn(),
      handleSnapshotSelection: vi.fn(),
    };
    h.themes = { themesEnabled: true, themes: [], isFindingThemes: false, themeProgress: null, hasRunThemes: false, handleFindThemes: vi.fn() };
    h.readout = { sourceCounts: { all: 2, web: 0, local: 2 }, identity: { buckets: [] }, headerMeta: '2 documents · 2 local', scopeName: 'All documents' };
    h.savedSearchDialog = {
      savedSearchNameDialogMode: null,
      savedSearchNameDialogValue: '',
      setSavedSearchNameDialogValue: vi.fn(),
      openSavedSearchNameDialog: vi.fn(),
      closeSavedSearchNameDialog: vi.fn(),
      submitSavedSearchNameDialog: vi.fn(),
    };
  });

  it('shows the library header, scope and count', () => {
    render(<FileBrowser />);
    expect(screen.getByRole('heading', { name: 'Library' })).toBeInTheDocument();
    expect(screen.getByText('2 documents · 2 local')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'All documents' })).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
    expect(screen.getByTestId('ListView')).toBeInTheDocument();
    expect(screen.queryByTestId('SelectionBar')).not.toBeInTheDocument();
    expect(preview().isOpen).toBe(false);
  });

  it.each(['tree', 'grid'])('renders the %s view', (viewMode) => {
    h.store = makeStore({ viewMode });
    render(<FileBrowser />);
    expect(screen.getByTestId(viewMode === 'tree' ? 'TreeView' : 'GridView')).toBeInTheDocument();
    expect(screen.queryByTestId('ListView')).not.toBeInTheDocument();
  });

  it('refreshes the library', async () => {
    render(<FileBrowser />);
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(h.query.refreshFiles).toHaveBeenCalled());
  });

  it('hides and shows the rail', () => {
    render(<FileBrowser />);
    expect(screen.getByTestId('LibraryRail')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Toggle rail' }));
    expect(screen.queryByTestId('LibraryRail')).not.toBeInTheDocument();
  });

  it('reports collections that failed to load and retries', () => {
    h.query.collectionsError = 'database locked';
    render(<FileBrowser />);
    expect(screen.getByRole('alert')).toHaveTextContent('Your collections could not be loaded: database locked');
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(h.query.refreshCollections).toHaveBeenCalled();
  });

  it('sorts by field and order together', () => {
    render(<FileBrowser />);
    h.props.LibraryToolbar.onSortChange('modifiedAt', 'desc');
    expect(h.store.setSortField).toHaveBeenCalledWith('modifiedAt');
    expect(h.store.setSortOrder).toHaveBeenCalledWith('desc');
  });

  it('sends Add folder to the import page', () => {
    render(<FileBrowser />);
    h.props.ListView.onAddFolder();
    expect(h.navigate).toHaveBeenCalledWith('/ingest');
  });

  it('opens the context menu at the pointer, or under the row from the keyboard', () => {
    render(<FileBrowser />);
    const preventDefault = vi.fn();
    const currentTarget = { getBoundingClientRect: () => ({ left: 12, bottom: 80 }) };
    h.props.ListView.onContextMenu({ preventDefault, clientX: 300, clientY: 200, currentTarget }, docs[0]);
    expect(h.store.openContextMenu).toHaveBeenCalledWith({ x: 300, y: 200 }, docs[0]);
    h.props.ListView.onContextMenu({ preventDefault, clientX: 0, clientY: 0, currentTarget }, docs[0]);
    expect(h.store.openContextMenu).toHaveBeenLastCalledWith({ x: 12, y: 80 }, docs[0]);
    expect(preventDefault).toHaveBeenCalledTimes(2);
  });

  describe('saved searches', () => {
    it('refuses to save with nothing searched or filtered', () => {
      render(<FileBrowser />);
      h.props.LibraryRail.onCreateSavedSearch('Empty');
      expect(h.toast.warning).toHaveBeenCalledWith('Nothing to save', { message: 'Search or filter first.' });
      expect(h.store.createSavedSearch).not.toHaveBeenCalled();
    });

    it('saves a search, and warns on a duplicate name', () => {
      h.store = makeStore({ searchQuery: 'saffron' });
      render(<FileBrowser />);
      h.props.LibraryRail.onCreateSavedSearch('Saffron');
      expect(h.store.createSavedSearch).toHaveBeenCalledWith('Saffron');
      expect(h.toast.warning).not.toHaveBeenCalled();
      h.store.createSavedSearch.mockReturnValueOnce(null);
      h.props.LibraryRail.onCreateSavedSearch('Saffron');
      expect(h.toast.warning).toHaveBeenCalledWith('Saved search not created', { message: 'That name is already in use.' });
    });

    it('counts a source filter or type facet as something to save', () => {
      h.store = makeStore({ filterBySource: 'web' });
      const first = render(<FileBrowser />);
      h.props.LibraryRail.onCreateSavedSearch('Web only');
      expect(h.store.createSavedSearch).toHaveBeenCalledTimes(1);
      first.unmount();
      h.store = makeStore({ activeFilter: 'pdf' });
      render(<FileBrowser />);
      h.props.LibraryRail.onCreateSavedSearch('PDFs');
      expect(h.store.createSavedSearch).toHaveBeenCalledTimes(1);
    });

    it('renames a saved search through the name dialog', () => {
      render(<FileBrowser />);
      h.props.LibraryRail.onRenameSavedSearch('s1', 'Old name');
      expect(h.savedSearchDialog.openSavedSearchNameDialog).toHaveBeenCalledWith('rename', 'Old name', 's1');
      expect(h.props.SavedSearchNameDialog.open).toBe(false);
    });
  });

  describe('renaming', () => {
    it('opens the rename dialog and refreshes after a rename', async () => {
      render(<FileBrowser />);
      act(() => h.props.ListView.onRename(docs[0]));
      expect(h.props.RenameDialog.document).toBe(docs[0]);
      fireEvent.click(screen.getByRole('button', { name: 'Renamed' }));
      await waitFor(() => expect(h.query.refreshFiles).toHaveBeenCalled());
      fireEvent.click(screen.getByRole('button', { name: 'Close rename' }));
      expect(screen.queryByRole('dialog', { name: 'Rename' })).not.toBeInTheDocument();
    });
  });

  describe('opening a document', () => {
    const open = async (target: DocumentMetadata) => {
      await act(async () => { h.props.ListView.onFileOpen(target); });
    };

    it('previews a supported local file at its path', async () => {
      render(<FileBrowser />);
      await open(docs[0]);
      expect(h.store.setFocusedDocument).toHaveBeenCalledWith('herbs');
      await waitFor(() => expect(preview().isOpen).toBe(true));
      expect(preview().source.filePath).toBe('/library/herbs.md');
      act(() => preview().onClose());
      expect(preview().isOpen).toBe(false);
    });

    it('previews a web page from its address', async () => {
      render(<FileBrowser />);
      await open(doc('bread', { filePath: 'https://example.org/bread', category: 'web article', fileType: 'html' }));
      await waitFor(() => expect(preview().source?.filePath).toBe('https://example.org/bread'));
      expect(h.openDocumentFile).not.toHaveBeenCalled();
    });

    it('previews an archived web page from its saved copy', async () => {
      h.openDocumentFile.mockResolvedValueOnce({ ok: true, data: { action: 'render_internal', contentPath: '/archive/bread.html' } });
      render(<FileBrowser />);
      await open(doc('bread', { filePath: '/data/.lattice/web-archive/bread', category: 'web', fileType: 'html' }));
      await waitFor(() => expect(preview().source?.filePath).toBe('/archive/bread.html'));
    });

    it('falls back to the stored path when the archive cannot be rendered', async () => {
      h.openDocumentFile.mockResolvedValueOnce({ ok: false, error: 'missing' });
      render(<FileBrowser />);
      await open(doc('bread', { filePath: '/data/.lattice/web-archive/bread', category: 'web', fileType: 'html' }));
      await waitFor(() => expect(preview().source?.filePath).toBe('/data/.lattice/web-archive/bread'));
    });

    it('resolves a relative path before previewing', async () => {
      h.resolveDocumentPath.mockResolvedValueOnce('/Users/ada/library/herbs.md');
      render(<FileBrowser />);
      await open(doc('herbs', { filePath: 'library/herbs.md' }));
      await waitFor(() => expect(preview().source?.filePath).toBe('/Users/ada/library/herbs.md'));
    });

    it('previews by id when a relative path cannot be resolved', async () => {
      h.resolveDocumentPath.mockResolvedValueOnce(null);
      render(<FileBrowser />);
      await open(doc('herbs', { filePath: 'library/herbs.md' }));
      await waitFor(() => expect(preview().source?.filePath).toBe('library/herbs.md'));
    });

    it('says so when a document has neither a path nor an id', async () => {
      h.resolveDocumentPath.mockResolvedValueOnce(null);
      render(<FileBrowser />);
      await open(doc('', { filePath: 'library/herbs.md' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith('Failed to resolve file path', { message: 'No file path for this document.' }));
      expect(preview().isOpen).toBe(false);
    });

    it('hands an unsupported file to the system, reporting a failure', async () => {
      h.openDocumentFile.mockResolvedValueOnce({ ok: true, data: { action: 'opened_external' } });
      render(<FileBrowser />);
      await open(doc('map', { filePath: '/library/map.xyz', fileType: 'xyz' }));
      expect(h.openDocumentFile).toHaveBeenCalledWith('map');
      expect(h.toast.error).not.toHaveBeenCalled();
      h.openDocumentFile.mockResolvedValueOnce({ ok: false, error: 'no app' });
      await open(doc('map', { filePath: '/library/map.xyz', fileType: 'xyz' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith('Failed to open file', { message: 'no app' }));
    });
  });

  it('passes ask-about and the focused document to the palette', () => {
    h.store = makeStore({ focusedDocumentId: 'spices' });
    render(<FileBrowser />);
    expect(h.paletteArgs.focusedDoc).toBe(docs[1]);
    h.paletteArgs.askAbout(docs[1]);
    expect(h.navigate).toHaveBeenCalledWith('/chat?new=1&documentId=spices');
  });

  describe('neighbourhood panel', () => {
    it('opens beside the focused document and follows its links', async () => {
      h.store = makeStore({ focusedDocumentId: 'herbs', isNeighborhoodOpen: true });
      render(<FileBrowser />);
      const panel = h.props.NeighborhoodPanel;
      expect(panel).toMatchObject({ documentId: 'herbs', documentTitle: 'herbs.md' });
      panel.onOpenConversation('c7');
      expect(h.navigate).toHaveBeenCalledWith('/chat?conversationId=c7');
      await act(async () => { panel.onOpenDocument('spices'); });
      await waitFor(() => expect(preview().source?.filePath).toBe('/library/spices.md'));
      panel.onOpenDocument('missing');
      expect(h.store.setFocusedDocument).toHaveBeenCalledTimes(1);
    });

    it('stays closed when the focused document is gone', () => {
      h.store = makeStore({ focusedDocumentId: 'deleted', isNeighborhoodOpen: true });
      render(<FileBrowser />);
      expect(screen.queryByTestId('NeighborhoodPanel')).not.toBeInTheDocument();
    });
  });

  describe('selection', () => {
    it('shows the selection bar and wires its actions', () => {
      h.store = makeStore({ selectedDocumentIds: new Set(['herbs']) });
      render(<FileBrowser />);
      const bar = h.props.SelectionBar;
      expect(bar.selectedCount).toBe(1);
      expect(bar.onRemoveFromCollection).toBeUndefined();
      bar.onAddToCollection();
      expect(h.collections.setCollectionSelection).toHaveBeenCalledWith(['herbs']);
      bar.onDelete();
      expect(h.deletion.setPendingBulkDelete).toHaveBeenCalledWith({ ids: ['herbs'], count: 1 });
      bar.onSelectAll();
      expect(h.store.selectAll).toHaveBeenCalledWith(['herbs', 'spices']);
      expect(bar.onClear).toBe(h.store.clearSelection);
      expect(bar.onSnapshot).toBe(h.collections.handleSnapshotSelection);
    });

    it('offers removal when the selection is in the open manual collection', () => {
      h.store = makeStore({ selectedDocumentIds: new Set(['herbs']), scope: { kind: 'collection', id: 'k1' } });
      h.collections.activeCollection = { id: 'k1', name: 'Kitchen', kind: 'manual', documentIds: ['herbs'] };
      h.collections.collectionCounts = new Map([['k1', 1]]);
      render(<FileBrowser />);
      h.props.SelectionBar.onRemoveFromCollection();
      expect(h.collections.handleRemoveFromCollection).toHaveBeenCalledWith(['herbs']);
      fireEvent.click(screen.getByRole('button', { name: 'Add documents' }));
      expect(h.collections.setAddDocumentsCollectionId).toHaveBeenCalledWith('k1');
    });
  });

  describe('empty collections', () => {
    it('invites documents into an empty manual collection', () => {
      h.collections.activeCollection = { id: 'k1', name: 'Kitchen', kind: 'manual', documentIds: [] };
      h.collections.collectionCounts = new Map([['k1', 0]]);
      render(<FileBrowser />);
      expect(screen.getByText('This collection is empty.')).toBeInTheDocument();
      expect(screen.getByText('Add documents from your Library to get started.')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Choose documents' }));
      expect(h.collections.setAddDocumentsCollectionId).toHaveBeenCalledWith('k1');
      expect(screen.queryByTestId('ListView')).not.toBeInTheDocument();
    });

    it('explains an empty snapshot without offering to fill it', () => {
      h.collections.activeCollection = { id: 'k2', name: 'Snapshot', kind: 'snapshot', documentIds: ['gone'] };
      h.collections.collectionCounts = new Map([['k2', 0]]);
      render(<FileBrowser />);
      expect(screen.getByText('The documents in this snapshot are no longer in your Library.')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Choose documents' })).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Add documents' })).not.toBeInTheDocument();
    });
  });

  describe('context menu', () => {
    it('opens for the chosen document and wires its actions', async () => {
      h.store = makeStore({ contextMenuPosition: { x: 1, y: 2 }, contextMenuDocument: docs[0] });
      render(<FileBrowser />);
      const menu = h.props.ContextMenu;
      expect(menu.doc).toBe(docs[0]);
      expect(menu.onClose).toBe(h.store.closeContextMenu);
      expect(menu.onRemoveFromCollection).toBeUndefined();
      menu.onAddToCollection(docs[0]);
      expect(h.collections.setCollectionSelection).toHaveBeenCalledWith(['herbs']);
      act(() => menu.onRename(docs[0]));
      expect(h.props.RenameDialog.document).toBe(docs[0]);
      await act(async () => { menu.onViewInRecall(docs[0]); });
      await waitFor(() => expect(preview().isOpen).toBe(true));
      menu.onDelete(docs[0]);
      expect(h.deletion.handleDelete).toHaveBeenCalledWith(docs[0]);
    });

    it('offers removal for a document in the open manual collection', () => {
      h.store = makeStore({ contextMenuPosition: { x: 1, y: 2 }, contextMenuDocument: docs[0] });
      h.collections.activeCollection = { id: 'k1', name: 'Kitchen', kind: 'manual', documentIds: ['herbs'] };
      h.collections.collectionCounts = new Map([['k1', 1]]);
      render(<FileBrowser />);
      h.props.ContextMenu.onRemoveFromCollection(docs[0]);
      expect(h.collections.handleRemoveFromCollection).toHaveBeenCalledWith(['herbs']);
    });
  });
});
