/**
 * Library — the corpus surface.
 *
 * Header (title + one data line + view toggles), one toolbar row, an optional
 * rail of the things the corpus is organised by, and the documents themselves
 * as hairline rows. No cards, no badges, no explanatory copy.
 */

import { useState, useCallback, useMemo } from 'react';

import { open as openExternal } from '@tauri-apps/plugin-shell';
import { useNavigate } from 'react-router';

import { EmptyState } from '@/components/EmptyState/EmptyState';
import { PageHeader } from '@/components/ui/PageHeader';
import {
  openDocumentFile,
  resolveDocumentPath,
  useLibraryDocumentActions,
} from '@/features/files/hooks/useLibraryDocumentActions';
import { useLibraryDocumentsQuery } from '@/features/files/hooks/useLibraryDocumentsQuery';
import { isHttpUrl, isWebDocument } from '@/features/files/model/documentMetadata';
import { useFileBrowserStore } from '@/features/files/stores/fileBrowserStore';
import { ContentViewer } from '@/features/reading/components/ContentViewer';
import { toast } from '@/stores/toastStore';
import { type DocumentMetadata, type SortField, type SortOrder } from '@/types/fileBrowser';
import { isSupportedFileType } from '@/utils/fileTypeDetector';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { ContextMenu } from './ContextMenu';
import { isAbsolutePath } from './fileBrowserHelpers';
import { GridView } from './GridView';
import { LibraryCollectionDialogs } from './LibraryCollectionDialogs';
import { LibraryDeleteDialogs } from './LibraryDeleteDialogs';
import { LibraryRail } from './LibraryRail';
import { LibraryToolbar } from './LibraryToolbar';
import { LibraryViewActions } from './LibraryViewActions';
import { ListView } from './ListView';
import { RenameDialog } from './RenameDialog';
import { SavedSearchNameDialog } from './SavedSearchNameDialog';
import { SelectionBar } from './SelectionBar';
import { TreeView } from './TreeView';
import { TypeFacets } from './TypeFacets';
import { useLibraryCollections } from './useLibraryCollections';
import { useLibraryContentSearch } from './useLibraryContentSearch';
import { useLibraryDeletion } from './useLibraryDeletion';
import { useLibraryDocumentCommands } from './useLibraryDocumentCommands';
import { useLibraryPaletteCommands } from './useLibraryPaletteCommands';
import { useLibraryReadout } from './useLibraryReadout';
import { useLibraryThemes } from './useLibraryThemes';
import { useSavedSearchNameDialog } from './useSavedSearchNameDialog';
import { NeighborhoodPanel } from '../Neighborhood';

export function FileBrowser() {
  const { documents, filteredDocuments, refreshFiles, collections: customCollections, collectionsError, refreshCollections } = useLibraryDocumentsQuery();
  const viewMode = useFileBrowserStore(state => state.viewMode);
  const setViewMode = useFileBrowserStore(state => state.setViewMode);
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const selectAll = useFileBrowserStore(state => state.selectAll);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const scope = useFileBrowserStore(state => state.scope);
  const setScope = useFileBrowserStore(state => state.setScope);
  const sourceConnections = useFileBrowserStore(state => state.sourceConnections);
  const searchQuery = useFileBrowserStore(state => state.searchQuery);
  const setSearchQuery = useFileBrowserStore(state => state.setSearchQuery);
  const filterByType = useFileBrowserStore(state => state.filterByType);
  const setFilterByType = useFileBrowserStore(state => state.setFilterByType);
  const filterBySource = useFileBrowserStore(state => state.filterBySource);
  const setFilterBySource = useFileBrowserStore(state => state.setFilterBySource);
  const sortField = useFileBrowserStore(state => state.sortField);
  const sortOrder = useFileBrowserStore(state => state.sortOrder);
  const setSortField = useFileBrowserStore(state => state.setSortField);
  const setSortOrder = useFileBrowserStore(state => state.setSortOrder);
  const groupByDate = useFileBrowserStore(state => state.groupByDate);
  const toggleGroupByDate = useFileBrowserStore(state => state.toggleGroupByDate);
  const savedSearches = useFileBrowserStore(state => state.savedSearches);
  const activeSavedSearchId = useFileBrowserStore(state => state.activeSavedSearchId);
  const createSavedSearch = useFileBrowserStore(state => state.createSavedSearch);
  const applySavedSearch = useFileBrowserStore(state => state.applySavedSearch);
  const isContentSearchLoading = useFileBrowserStore(state => state.isContentSearchLoading);
  const contextMenuPosition = useFileBrowserStore(state => state.contextMenuPosition);
  const contextMenuDocument = useFileBrowserStore(state => state.contextMenuDocument);
  const openContextMenu = useFileBrowserStore(state => state.openContextMenu);
  const closeContextMenu = useFileBrowserStore(state => state.closeContextMenu);
  const focusedDocumentId = useFileBrowserStore(state => state.focusedDocumentId);
  const setFocusedDocument = useFileBrowserStore(state => state.setFocusedDocument);
  const isNeighborhoodOpen = useFileBrowserStore(state => state.isNeighborhoodOpen);
  const toggleNeighborhood = useFileBrowserStore(state => state.toggleNeighborhood);

  const [isRailOpen, setIsRailOpen] = useState(true);
  const [viewerFilePath, setViewerFilePath] = useState<string | null>(null);
  const [renameDocument, setRenameDocument] = useState<DocumentMetadata | null>(null);
  const {
    savedSearchNameDialogMode,
    savedSearchNameDialogValue,
    setSavedSearchNameDialogValue,
    openSavedSearchNameDialog,
    closeSavedSearchNameDialog,
    submitSavedSearchNameDialog,
  } = useSavedSearchNameDialog();
  const documentActions = useLibraryDocumentActions();
  const deletion = useLibraryDeletion({ deleteDocuments: documentActions.deleteDocuments, clearSelection });
  const { handleDelete, setPendingBulkDelete } = deletion;
  const {
    spaces,
    isLoadingSpaces,
    isAssigningSpace,
    reindexDocument,
    removeFromIndex,
    handleBulkAssignToSpace,
  } = useLibraryDocumentCommands({ documentActions, selectedDocumentIds, clearSelection });
  const collections = useLibraryCollections({
    documents,
    customCollections,
    scope,
    setScope,
    selectedDocumentIds,
    clearSelection,
    searchQuery,
    setSearchQuery,
    setFilterByType,
    setFilterBySource,
  });
  const {
    activeCollection,
    collectionCounts,
    setCollectionSelection,
    setAddDocumentsCollectionId,
    setRenameCollection,
    setPendingDeleteCollection,
    handleCreateCollection,
    handleRemoveFromCollection,
    handleSnapshotSelection,
  } = collections;

  const navigate = useNavigate();

  const documentsById = useMemo(
    () => new Map(documents.map((doc) => [doc.id, doc])),
    [documents]
  );
  const focusedDoc = useMemo(
    () => (focusedDocumentId ? documentsById.get(focusedDocumentId) ?? null : null),
    [documentsById, focusedDocumentId]
  );

  const {
    themesEnabled,
    themes,
    isFindingThemes,
    themeProgress,
    hasRunThemes,
    handleFindThemes,
  } = useLibraryThemes({ documents, scope, setScope });
  const { sourceCounts, identity, headerMeta, scopeName } = useLibraryReadout({
    documents,
    customCollections,
    scope,
    filterBySource,
    themes,
  });

  const { activeFilter } = useLibraryContentSearch(documents, searchQuery, filterByType);

  const handleFileOpen = useCallback(async (doc: DocumentMetadata) => {
    setFocusedDocument(doc.id);
    if (isWebDocument(doc)) {
      if (isHttpUrl(doc.filePath)) {
        try {
          await openExternal(doc.filePath);
        } catch (error) {
          toast.error('Failed to open URL', {
            message: error instanceof Error ? error.message : String(error),
          });
        }
        return;
      }

      const webOpen = await openDocumentFile(doc.id);
      if (!webOpen.ok) {
        toast.error('Failed to open web article', { message: webOpen.error });
        return;
      }

      if (webOpen.data.action === 'render_internal') {
        setViewerFilePath(webOpen.data.contentPath);
      }
      return;
    }

    if (isSupportedFileType(doc.filePath)) {
      if (isAbsolutePath(doc.filePath)) {
        setViewerFilePath(doc.filePath);
        return;
      }

      const resolvedPath = await resolveDocumentPath(doc);
      if (resolvedPath) {
        setViewerFilePath(resolvedPath);
        return;
      }

      toast.error('Failed to resolve file path', { message: 'No file path for this document.' });
      return;
    }

    const openById = await openDocumentFile(doc.id);
    if (!openById.ok) {
      toast.error('Failed to open file', { message: openById.error });
    }
  }, [setFocusedDocument]);

  const askAbout = useCallback(
    (doc: DocumentMetadata) => {
      navigate(`/chat?${new URLSearchParams({ new: '1', documentId: doc.id }).toString()}`);
    },
    [navigate]
  );


  const openConversation = useCallback(
    (conversationId: string) => {
      navigate(`/chat?${new URLSearchParams({ conversationId }).toString()}`);
    },
    [navigate]
  );

  const openDocumentById = useCallback(
    (documentId: string) => {
      const doc = documentsById.get(documentId);
      if (doc) void handleFileOpen(doc);
    },
    [documentsById, handleFileOpen]
  );

  useLibraryPaletteCommands({
    focusedDoc,
    askAbout,
    reindexDocument,
    removeFromIndex,
    isNeighborhoodOpen,
    toggleNeighborhood,
    hasRunThemes,
    themesEnabled,
    isFindingThemes,
    handleFindThemes,
  });

  const handleContextMenu = useCallback(
    (event: React.MouseEvent, doc: DocumentMetadata) => {
      event.preventDefault();
      const rect = event.currentTarget.getBoundingClientRect();
      openContextMenu({
        x: event.clientX || rect.left,
        y: event.clientY || rect.bottom,
      }, doc);
    },
    [openContextMenu]
  );

  const handleRefresh = useCallback(async () => {
    await refreshFiles();
  }, [refreshFiles]);

  const handleAddFolder = useCallback(() => {
    navigate('/ingest');
  }, [navigate]);

  const handleSortChange = useCallback((field: SortField, order: SortOrder) => {
    setSortField(field);
    setSortOrder(order);
  }, [setSortField, setSortOrder]);

  const handleCreateSavedSearch = useCallback((name: string) => {
    const hasCriteria = Boolean(searchQuery.trim()) || Boolean(activeFilter) || filterBySource !== 'all';
    if (!hasCriteria) {
      toast.warning('Nothing to save', { message: 'Search or filter first.' });
      return;
    }

    const searchId = createSavedSearch(name);
    if (!searchId) {
      toast.warning('Saved search not created', { message: 'That name is already in use.' });
    }
  }, [activeFilter, createSavedSearch, filterBySource, searchQuery]);


  const handleRename = useCallback((doc: DocumentMetadata) => {
    setRenameDocument(doc);
  }, []);

  const handleRenameSuccess = useCallback(async () => {
    await refreshFiles();
  }, [refreshFiles]);

  const viewProps = {
    onFileOpen: handleAsyncEvent(handleFileOpen),
    onContextMenu: handleContextMenu,
    onRename: handleRename,
    onDelete: handleAsyncEvent(handleDelete),
    onAddFolder: handleAddFolder,
  };

  return (
    <main className="h-full overflow-y-auto bg-bg">
      <div
        className={`mx-auto flex h-full w-full flex-col px-6 pb-10 pt-10 ${
          isNeighborhoodOpen && focusedDoc ? 'max-w-[1360px]' : 'max-w-[1100px]'
        }`}
      >
        <PageHeader
          title="Library"
          meta={headerMeta}
          actions={
            <LibraryViewActions
              viewMode={viewMode}
              setViewMode={setViewMode}
              isNeighborhoodOpen={isNeighborhoodOpen}
              focusedDocumentId={focusedDocumentId}
              toggleNeighborhood={toggleNeighborhood}
              handleRefresh={handleRefresh}
            />
          }
        />

        <LibraryToolbar
          isRailOpen={isRailOpen}
          onToggleRail={() => setIsRailOpen((open) => !open)}
          searchQuery={searchQuery}
          onSearchQueryChange={setSearchQuery}
          isContentSearchLoading={isContentSearchLoading}
          sourceCounts={sourceCounts}
          filterBySource={filterBySource}
          onFilterBySourceChange={setFilterBySource}
          sortField={sortField}
          sortOrder={sortOrder}
          onSortChange={handleSortChange}
          groupByDate={groupByDate}
          onToggleGroupByDate={toggleGroupByDate}
        />

        {collectionsError ? (
          <div role="alert" className="mt-3 flex items-center justify-between rounded-md border border-danger/30 bg-danger/5 px-3 py-2 text-sm text-danger">
            <span>Your collections could not be loaded: {collectionsError}</span>
            <button type="button" className="underline" onClick={() => { void refreshCollections(); }}>Retry</button>
          </div>
        ) : null}

        <div className="flex min-h-0 flex-1 gap-6">
          {isRailOpen ? (
            <LibraryRail
              scope={scope}
              onScopeChange={setScope}
              collections={customCollections}
              onCreateCollection={handleCreateCollection}
              onRenameCollection={setRenameCollection}
              onDeleteCollection={setPendingDeleteCollection}
              collectionCounts={collectionCounts}
              savedSearches={savedSearches}
              activeSavedSearchId={activeSavedSearchId}
              onApplySavedSearch={applySavedSearch}
              onRenameSavedSearch={(searchId, name) =>
                openSavedSearchNameDialog('rename', name, searchId)
              }
              onCreateSavedSearch={handleCreateSavedSearch}
              sources={sourceConnections}
              themes={themes}
              themesEnabled={themesEnabled}
              hasRunThemes={hasRunThemes}
              isFindingThemes={isFindingThemes}
              themeProgress={themeProgress}
              onFindThemes={handleFindThemes}
            />
          ) : null}

          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <div className="flex items-baseline gap-2 pb-3">
              <h2 className="min-w-0 truncate text-lg font-medium text-text-secondary">{scopeName}</h2>
              <span className="text-sm tabular-nums text-text-muted">
                {filteredDocuments.length.toLocaleString()}
              </span>
              {activeCollection?.kind === 'manual' ? (
                <button type="button" onClick={() => setAddDocumentsCollectionId(activeCollection.id)} className="ml-auto shrink-0 rounded-sm border border-border-default px-3 py-1.5 text-sm text-text-primary hover:bg-surface">Add documents</button>
              ) : null}
            </div>
            <TypeFacets
              buckets={identity.buckets}
              activeType={filterByType}
              onSelect={setFilterByType}
            />

            {selectedDocumentIds.size > 0 ? (
              <SelectionBar
                selectedCount={selectedDocumentIds.size}
                spaces={spaces}
                isLoadingSpaces={isLoadingSpaces}
                isAssigningSpace={isAssigningSpace}
                onAssignToSpace={handleAsyncEvent(handleBulkAssignToSpace)}
                onAddToCollection={() => setCollectionSelection(Array.from(selectedDocumentIds))}
                onRemoveFromCollection={activeCollection?.kind === 'manual' && activeCollection.documentIds.some(id => selectedDocumentIds.has(id))
                  ? () => handleRemoveFromCollection(Array.from(selectedDocumentIds)) : undefined}
                onSnapshot={handleSnapshotSelection}
                onDelete={() =>
                  setPendingBulkDelete({
                    ids: Array.from(selectedDocumentIds),
                    count: selectedDocumentIds.size,
                  })
                }
                onSelectAll={() => selectAll(filteredDocuments.map((doc) => doc.id))}
                onClear={clearSelection}
              />
            ) : null}

            <div className="min-h-0 flex-1">
              {activeCollection && collectionCounts.get(activeCollection.id) === 0 ? (
                <EmptyState
                  title="This collection is empty."
                  description={activeCollection.kind === 'manual' ? 'Add documents from your Library to get started.' : 'The documents in this snapshot are no longer in your Library.'}
                  action={activeCollection.kind === 'manual' ? { label: 'Choose documents', onClick: () => setAddDocumentsCollectionId(activeCollection.id) } : undefined}
                />
              ) : <>
              {viewMode === 'tree' && <TreeView {...viewProps} />}
              {viewMode === 'list' && <ListView {...viewProps} />}
              {viewMode === 'grid' && (
                <GridView
                  onFileOpen={viewProps.onFileOpen}
                  onContextMenu={viewProps.onContextMenu}
                  onAddFolder={viewProps.onAddFolder}
                />
              )}
              </>}
            </div>
          </div>

          {isNeighborhoodOpen && focusedDoc ? (
            <NeighborhoodPanel
              documentId={focusedDoc.id}
              documentTitle={focusedDoc.fileName}
              documentsById={documentsById}
              onOpenDocument={openDocumentById}
              onOpenConversation={openConversation}
              onClose={toggleNeighborhood}
            />
          ) : null}
        </div>
      </div>

      {contextMenuPosition && contextMenuDocument && (
        <ContextMenu
          doc={contextMenuDocument}
          position={contextMenuPosition}
          onClose={closeContextMenu}
          onViewInRecall={(doc) => {
            void handleFileOpen(doc);
          }}
          onRename={handleRename}
          onDelete={handleAsyncEvent(handleDelete)}
          onAddToCollection={doc => setCollectionSelection([doc.id])}
          onRemoveFromCollection={activeCollection?.kind === 'manual' && activeCollection.documentIds.includes(contextMenuDocument.id)
            ? doc => handleRemoveFromCollection([doc.id]) : undefined}
        />
      )}

      {renameDocument && (
        <RenameDialog
          document={renameDocument}
          onClose={() => setRenameDocument(null)}
          onSuccess={handleAsyncEvent(handleRenameSuccess)}
        />
      )}

      <SavedSearchNameDialog
        open={savedSearchNameDialogMode !== null}
        mode={savedSearchNameDialogMode}
        value={savedSearchNameDialogValue}
        onChangeValue={setSavedSearchNameDialogValue}
        onClose={closeSavedSearchNameDialog}
        onSubmit={submitSavedSearchNameDialog}
      />

      <ContentViewer filePath={viewerFilePath} onClose={() => setViewerFilePath(null)} />

      <LibraryCollectionDialogs collections={collections} documents={documents} />

      <LibraryDeleteDialogs deletion={deletion} />
    </main>
  );
}
