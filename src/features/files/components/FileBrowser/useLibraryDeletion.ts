import { useCallback, useRef, useState } from 'react';

import type { useLibraryDocumentActions } from '@/features/files/hooks/useLibraryDocumentActions';
import { toast } from '@/stores/toastStore';
import { type DocumentMetadata } from '@/types/fileBrowser';

import { errorMessage, plural } from './fileBrowserHelpers';

type DeleteDocumentsMutation = ReturnType<typeof useLibraryDocumentActions>['deleteDocuments'];

/** Pending single and bulk deletes, their progress, and the confirm handlers. */
export function useLibraryDeletion({
  deleteDocuments,
  clearSelection,
}: {
  deleteDocuments: DeleteDocumentsMutation;
  clearSelection: () => void;
}) {
  const [pendingDeleteDoc, setPendingDeleteDoc] = useState<DocumentMetadata | null>(null);
  const [pendingBulkDelete, setPendingBulkDelete] = useState<{ ids: string[]; count: number } | null>(null);
  const bulkDeleteCancelled = useRef(false);
  const [deleteProgress, setDeleteProgress] = useState(0);
  const isDeleting = deleteDocuments.isPending;

  const executeBulkDelete = useCallback(async () => {
    if (!pendingBulkDelete || isDeleting) return;

    bulkDeleteCancelled.current = false;
    setDeleteProgress(0);

    try {
      const { deleted, errors } = await deleteDocuments.mutateAsync({
        ids: pendingBulkDelete.ids,
        shouldStop: () => bulkDeleteCancelled.current,
        onProgress: setDeleteProgress,
      });

      if (deleted > 0) {
        toast.success(`${plural(deleted, 'document')} deleted`);
        clearSelection();
      }

      if (errors.length > 0) {
        toast.error(`Failed to delete ${plural(errors.length, 'document')}`, {
          message: errors.slice(0, 3).join(', ') + (errors.length > 3 ? '…' : ''),
        });
      }

    } catch (error) {
      toast.error('Deletion stopped', { message: errorMessage(error) });
    } finally {
      setPendingBulkDelete(null);
    }
  }, [pendingBulkDelete, isDeleting, clearSelection, deleteDocuments]);

  const handleDelete = useCallback(async (doc: DocumentMetadata) => {
    setPendingDeleteDoc(doc);
  }, []);

  const executeDelete = useCallback(async () => {
    if (!pendingDeleteDoc) return;

    const { errors } = await deleteDocuments.mutateAsync({ ids: [pendingDeleteDoc.id] });
    if (errors.length > 0) throw new Error(errors[0]);
    toast.success('Document deleted');
    setPendingDeleteDoc(null);
  }, [deleteDocuments, pendingDeleteDoc]);

  return {
    pendingDeleteDoc,
    setPendingDeleteDoc,
    pendingBulkDelete,
    setPendingBulkDelete,
    bulkDeleteCancelled,
    deleteProgress,
    isDeleting,
    executeBulkDelete,
    handleDelete,
    executeDelete,
  };
}
