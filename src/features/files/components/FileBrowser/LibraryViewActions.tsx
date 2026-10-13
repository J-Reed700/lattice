import { LayoutGrid, List, ListTree, PanelRight, RefreshCw } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { type ViewMode } from '@/types/fileBrowser';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

/** The Library header's view-mode toggles, the related-documents toggle and refresh. */
export function LibraryViewActions({
  viewMode,
  setViewMode,
  isNeighborhoodOpen,
  focusedDocumentId,
  toggleNeighborhood,
  handleRefresh,
}: {
  viewMode: ViewMode;
  setViewMode: (mode: ViewMode) => void;
  isNeighborhoodOpen: boolean;
  focusedDocumentId: string | null;
  toggleNeighborhood: () => void;
  handleRefresh: () => Promise<void>;
}) {
  return (
    <>
      <IconButton
        label="List view"
        active={viewMode === 'list'}
        onClick={() => setViewMode('list')}
      >
        <List strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label="Grid view"
        active={viewMode === 'grid'}
        onClick={() => setViewMode('grid')}
      >
        <LayoutGrid strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label="Tree view"
        active={viewMode === 'tree'}
        onClick={() => setViewMode('tree')}
      >
        <ListTree strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label="Related"
        active={isNeighborhoodOpen}
        disabled={focusedDocumentId === null}
        onClick={toggleNeighborhood}
      >
        <PanelRight strokeWidth={1.75} />
      </IconButton>
      <IconButton label="Refresh" onClick={handleAsyncEvent(handleRefresh)}>
        <RefreshCw strokeWidth={1.75} />
      </IconButton>
    </>
  );
}
