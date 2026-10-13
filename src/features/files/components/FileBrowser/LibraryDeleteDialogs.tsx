import { ConfirmDialog } from '@/components/ConfirmDialog';

import { plural } from './fileBrowserHelpers';

import type { useLibraryDeletion } from './useLibraryDeletion';

/** The confirmations for deleting one document and for deleting the selection. */
export function LibraryDeleteDialogs({
  deletion,
}: {
  deletion: ReturnType<typeof useLibraryDeletion>;
}) {
  const {
    pendingDeleteDoc,
    setPendingDeleteDoc,
    pendingBulkDelete,
    setPendingBulkDelete,
    bulkDeleteCancelled,
    deleteProgress,
    isDeleting,
    executeBulkDelete,
    executeDelete,
  } = deletion;
  return (
    <>
      <ConfirmDialog
        isOpen={pendingDeleteDoc !== null}
        title="Delete document?"
        message={`Delete "${pendingDeleteDoc?.fileName ?? 'this document'}"? The document and its embeddings are removed from the index.`}
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={isDeleting}
        onConfirm={executeDelete}
        onCancel={() => setPendingDeleteDoc(null)}
      />

      <ConfirmDialog
        isOpen={pendingBulkDelete !== null}
        title="Delete documents?"
        message={isDeleting
          ? `Processed ${deleteProgress} of ${pendingBulkDelete?.count ?? 0} documents. Cancel stops after the current document finishes.`
          : `Delete ${plural(pendingBulkDelete?.count ?? 0, 'document')}? They and their embeddings are removed from the index.`}
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={isDeleting}
        onConfirm={executeBulkDelete}
        allowCancelWhileLoading
        onCancel={() => {
          bulkDeleteCancelled.current = true;
          setPendingBulkDelete(null);
        }}
      />
    </>
  );
}
