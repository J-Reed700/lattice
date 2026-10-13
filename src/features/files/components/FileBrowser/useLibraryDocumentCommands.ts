import { useCallback, useMemo } from 'react';

import type { useLibraryDocumentActions } from '@/features/files/hooks/useLibraryDocumentActions';
import { useSpacesQuery } from '@/features/spaces/api/queries';
import { toast } from '@/stores/toastStore';
import { type DocumentMetadata, type FileBrowserActions } from '@/types/fileBrowser';

import { errorMessage, plural } from './fileBrowserHelpers';

/** Reindexing, unindexing, and adding the selection to a space, with their toasts. */
export function useLibraryDocumentCommands({
  documentActions,
  selectedDocumentIds,
  clearSelection,
}: {
  documentActions: ReturnType<typeof useLibraryDocumentActions>;
  selectedDocumentIds: Set<string>;
  clearSelection: FileBrowserActions['clearSelection'];
}) {

  const isAssigningSpace = documentActions.addToSpace.isPending;
  const spacesQuery = useSpacesQuery();
  // Open spaces first, then archived ones, each by name.
  const spaces = useMemo(
    () => (spacesQuery.data ?? [])
      .slice()
      .sort((a, b) => Number(a.isArchived) - Number(b.isArchived) || a.name.localeCompare(b.name)),
    [spacesQuery.data]
  );
  const isLoadingSpaces = spacesQuery.isLoading;

  const { reindex, removeFromIndex: unindex, addToSpace } = documentActions;

  const reindexDocument = useCallback(
    async (doc: DocumentMetadata) => {
      try {
        await reindex.mutateAsync(doc);
        toast.success(`Reindexing ${doc.fileName}`);
      } catch (error) {
        toast.error(`Couldn't reindex ${doc.fileName}`, { message: errorMessage(error) });
      }
    },
    [reindex]
  );

  const removeFromIndex = useCallback(
    async (doc: DocumentMetadata) => {
      try {
        await unindex.mutateAsync(doc);
        toast.success(`Removed ${doc.fileName} from the index`);
      } catch (error) {
        toast.error(`Couldn't remove ${doc.fileName} from the index`, { message: errorMessage(error) });
      }
    },
    [unindex]
  );

  const handleBulkAssignToSpace = useCallback(async (spaceId: string) => {
    const selectedIds = Array.from(selectedDocumentIds);
    if (selectedIds.length === 0) {
      return;
    }

    try {
      await addToSpace.mutateAsync({ documentIds: selectedIds, spaceId });
      const spaceName = spaces.find((space) => space.id === spaceId)?.name ?? 'space';
      toast.success(`${plural(selectedIds.length, 'document')} added to ${spaceName}`);
      clearSelection();
    } catch (error) {
      toast.error('Failed to add documents to space', { message: errorMessage(error) });
    }
  }, [addToSpace, clearSelection, selectedDocumentIds, spaces]);

  return {
    spaces,
    isLoadingSpaces,
    isAssigningSpace,
    reindexDocument,
    removeFromIndex,
    handleBulkAssignToSpace,
  };
}
