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
    <div className="flex items-center gap-1.5">
      <div
        role="group"
        aria-label="View mode"
        className="inline-flex h-8 items-center gap-0.5 rounded-lg border border-border-subtle bg-surface-sunken/70 p-[3px]"
      >
        <IconButton
          label="List view"
          active={viewMode === 'list'}
          onClick={() => setViewMode('list')}
          className={viewMode === 'list' ? 'h-full w-7 rounded-md bg-surface shadow-sm ring-1 ring-border-subtle' : 'h-full w-7 rounded-md'}
        >
          <List strokeWidth={1.75} />
        </IconButton>
        <IconButton
          label="Grid view"
          active={viewMode === 'grid'}
          onClick={() => setViewMode('grid')}
          className={viewMode === 'grid' ? 'h-full w-7 rounded-md bg-surface shadow-sm ring-1 ring-border-subtle' : 'h-full w-7 rounded-md'}
        >
          <LayoutGrid strokeWidth={1.75} />
        </IconButton>
        <IconButton
          label="Tree view"
          active={viewMode === 'tree'}
          onClick={() => setViewMode('tree')}
          className={viewMode === 'tree' ? 'h-full w-7 rounded-md bg-surface shadow-sm ring-1 ring-border-subtle' : 'h-full w-7 rounded-md'}
        >
          <ListTree strokeWidth={1.75} />
        </IconButton>
      </div>

      <IconButton
        label="Related"
        active={isNeighborhoodOpen}
        disabled={focusedDocumentId === null}
        onClick={toggleNeighborhood}
        className="h-8 w-8 rounded-lg"
      >
        <PanelRight strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label="Refresh"
        onClick={handleAsyncEvent(handleRefresh)}
        className="h-8 w-8 rounded-lg"
      >
        <RefreshCw strokeWidth={1.75} />
      </IconButton>
    </div>
  );
}
