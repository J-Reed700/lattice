import { ConfirmDialog } from '@/components/ConfirmDialog';
import { type DocumentMetadata } from '@/types/fileBrowser';

import { AddToCollectionDialog, CollectionDocumentsDialog, RenameCollectionDialog } from './CollectionDialogs';

import type { useLibraryCollections } from './useLibraryCollections';

/** The dialogs that add to, fill, rename and delete a collection. */
export function LibraryCollectionDialogs({
  collections,
  documents,
}: {
  collections: ReturnType<typeof useLibraryCollections>;
  documents: DocumentMetadata[];
}) {
  const {
    collectionSelection,
    setCollectionSelection,
    addDocumentsCollection,
    setAddDocumentsCollectionId,
    renameCollection,
    setRenameCollection,
    pendingDeleteCollection,
    setPendingDeleteCollection,
    confirmDeleteCollection,
  } = collections;
  return (
    <>
      {collectionSelection ? <AddToCollectionDialog documentIds={collectionSelection} onClose={() => setCollectionSelection(null)} /> : null}
      {addDocumentsCollection ? <CollectionDocumentsDialog collection={addDocumentsCollection} documents={documents} onClose={() => setAddDocumentsCollectionId(null)} /> : null}
      {renameCollection ? <RenameCollectionDialog collection={renameCollection} onClose={() => setRenameCollection(null)} /> : null}
      <ConfirmDialog
        isOpen={pendingDeleteCollection !== null}
        title="Delete collection?"
        message={`Delete “${pendingDeleteCollection?.name ?? ''}” and any collections inside it? Your documents will remain in the Library.`}
        confirmLabel="Delete collection"
        variant="danger"
        onConfirm={confirmDeleteCollection}
        onCancel={() => setPendingDeleteCollection(null)}
      />
    </>
  );
}
