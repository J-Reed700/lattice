import { useCallback, useEffect, useMemo, useState } from 'react';

import { useCustomCollectionActions } from '@/features/files/hooks/useCustomCollectionsQuery';
import { filterLibraryDocuments } from '@/features/files/stores/fileBrowserStore';
import { toast } from '@/stores/toastStore';
import {
  type CustomCollection,
  type DocumentMetadata,
  type FileBrowserActions,
  type LibraryScope,
} from '@/types/fileBrowser';

import { plural } from './fileBrowserHelpers';

/**
 * The Library's collections: the active one, per-collection counts, the
 * collection dialogs' state, and creating, snapshotting, removing from and
 * deleting collections.
 */
export function useLibraryCollections({
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
}: {
  documents: DocumentMetadata[];
  customCollections: CustomCollection[];
  scope: LibraryScope;
  selectedDocumentIds: Set<string>;
  searchQuery: string;
} & Pick<
  FileBrowserActions,
  'setScope' | 'clearSelection' | 'setSearchQuery' | 'setFilterByType' | 'setFilterBySource'
>) {
  const collectionActions = useCustomCollectionActions();
  const [collectionSelection, setCollectionSelection] = useState<string[] | null>(null);
  const [addDocumentsCollectionId, setAddDocumentsCollectionId] = useState<string | null>(null);
  const [renameCollection, setRenameCollection] = useState<CustomCollection | null>(null);
  const [pendingDeleteCollection, setPendingDeleteCollection] = useState<CustomCollection | null>(null);

  const activeCollection = scope.kind === 'collection'
    ? customCollections.find(collection => collection.id === scope.id) ?? null
    : null;
  const addDocumentsCollection = customCollections.find(collection => collection.id === addDocumentsCollectionId);
  const collectionCounts = useMemo(() => new Map(customCollections.map(collection => [
    collection.id,
    filterLibraryDocuments(documents, {
      searchQuery: '', filterByType: null, filterBySource: 'all', contentSearchMatches: new Set(),
      customCollections, scope: { kind: 'collection', id: collection.id },
    }).length,
  ])), [customCollections, documents]);

  useEffect(() => {
    if (scope.kind === 'collection' && !activeCollection) setScope({ kind: 'all' });
  }, [activeCollection, scope, setScope]);

  const handleCreateCollection = useCallback(async (name: string) => {
    let collectionId: string | null;
    try { collectionId = await collectionActions.create(name); }
    catch (error) { toast.error('Collection could not be saved', { message: error instanceof Error ? error.message : String(error) }); return; }
    if (!collectionId) {
      toast.warning('Collection not created', { message: 'That name is already in use.' });
      return;
    }
    const ids = Array.from(selectedDocumentIds);
    if (ids.length > 0) {
      try { await collectionActions.addDocuments(collectionId, ids); }
      catch (error) { toast.error('Documents could not be added to the collection', { message: error instanceof Error ? error.message : String(error) }); }
    }
    else setAddDocumentsCollectionId(collectionId);
    setSearchQuery('');
    setFilterByType(null);
    setFilterBySource('all');
    setScope({ kind: 'collection', id: collectionId });
  }, [collectionActions, selectedDocumentIds, setFilterBySource, setFilterByType, setScope, setSearchQuery]);

  const handleRemoveFromCollection = useCallback(async (ids: string[]) => {
    if (activeCollection?.kind !== 'manual') return;
    try { await collectionActions.removeDocuments(activeCollection.id, ids); }
    catch (error) { toast.error('Documents could not be removed from the collection', { message: error instanceof Error ? error.message : String(error) }); return; }
    clearSelection();
    toast.success(`Removed from ${activeCollection.name}`, { message: 'Documents are still in your Library.' });
  }, [activeCollection, clearSelection, collectionActions]);

  const handleSnapshotSelection = useCallback(async () => {
    const docIds = Array.from(selectedDocumentIds);
    if (docIds.length === 0) {
      return;
    }

    const label = searchQuery.trim() || new Date().toLocaleDateString();
    let collectionId: string | null;
    try { collectionId = await collectionActions.create(`Snapshot: ${label}`, null, docIds); }
    catch (error) { toast.error('Snapshot could not be saved', { message: error instanceof Error ? error.message : String(error) }); return; }
    if (!collectionId) {
      toast.warning('Snapshot not created', { message: 'That name is already in use.' });
      return;
    }

    toast.success(`${plural(docIds.length, 'document')} captured`);
    clearSelection();
  }, [clearSelection, collectionActions, searchQuery, selectedDocumentIds]);

  const confirmDeleteCollection = async () => {
    if (!pendingDeleteCollection) return;
    try { await collectionActions.deleteCollection(pendingDeleteCollection.id); setPendingDeleteCollection(null); }
    catch (error) { toast.error('Collection could not be deleted', { message: error instanceof Error ? error.message : String(error) }); }
  };

  return {
    activeCollection,
    addDocumentsCollection,
    collectionCounts,
    collectionSelection,
    setCollectionSelection,
    setAddDocumentsCollectionId,
    renameCollection,
    setRenameCollection,
    pendingDeleteCollection,
    setPendingDeleteCollection,
    handleCreateCollection,
    handleRemoveFromCollection,
    handleSnapshotSelection,
    confirmDeleteCollection,
  };
}
