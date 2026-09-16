/**
 * Grid view — tiles with a hairline border, a name, and one muted meta line.
 * No thumbnails-in-cards theatrics, no badges.
 */

import { useCallback, memo, useRef } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';

import { metaLine } from './docMeta';
import { CorpusEmptyState, ErrorState, FilterEmptyState } from './EmptyStates';
import { FileIcon } from './FileIcon';
import { useLibraryDocumentsQuery } from '../../hooks/queries/useLibraryDocumentsQuery';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';
import { type DocumentMetadata } from '../../types/fileBrowser';
import { FileBrowserSkeleton } from '../Skeleton';

interface GridViewProps {
  onFileOpen?: (_doc: DocumentMetadata) => void;
  onContextMenu?: (_event: React.MouseEvent, _doc: DocumentMetadata) => void;
  onAddFolder?: () => void;
}

const COLUMNS = 3;
const ROW_HEIGHT = 112;

export const GridView = memo(({ onFileOpen, onContextMenu, onAddFolder }: GridViewProps) => {
  const {
    documents: allDocuments,
    filteredDocuments: documents,
    isLoading,
    error,
  } = useLibraryDocumentsQuery();
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const selectFile = useFileBrowserStore(state => state.selectFile);
  const toggleSelection = useFileBrowserStore(state => state.toggleSelection);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const setFocusedDocument = useFileBrowserStore(state => state.setFocusedDocument);

  const parentRef = useRef<HTMLDivElement>(null);
  const rowCount = Math.ceil(documents.length / COLUMNS);

  const rowVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 3,
  });

  const handleCardClick = useCallback(
    (doc: DocumentMetadata, event: React.MouseEvent) => {
      setFocusedDocument(doc.id);
      if (event.metaKey || event.ctrlKey) {
        toggleSelection(doc.id);
      } else {
        clearSelection();
        selectFile(doc.id);
      }
    },
    [toggleSelection, selectFile, clearSelection, setFocusedDocument]
  );

  if (isLoading) {
    return <FileBrowserSkeleton view="grid" count={15} />;
  }

  if (error) {
    return <ErrorState message={error} />;
  }

  if (allDocuments.length === 0) {
    return <CorpusEmptyState onAddFolder={onAddFolder ?? (() => {})} />;
  }

  if (documents.length === 0) {
    return <FilterEmptyState />;
  }

  return (
    <div ref={parentRef} className="h-full overflow-y-auto border-t border-border-subtle pt-3">
      <div style={{ height: `${rowVirtualizer.getTotalSize()}px`, position: 'relative' }}>
        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
          const startIndex = virtualRow.index * COLUMNS;
          const rowDocs = documents.slice(startIndex, Math.min(startIndex + COLUMNS, documents.length));

          return (
            <div
              key={virtualRow.key}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              <div className="grid grid-cols-2 gap-3 px-1 md:grid-cols-3">
                {rowDocs.map((doc) => (
                  <FileCard
                    key={doc.id}
                    doc={doc}
                    isSelected={selectedDocumentIds.has(doc.id)}
                    onClick={handleCardClick}
                    onDoubleClick={() => onFileOpen?.(doc)}
                    onContextMenu={onContextMenu}
                  />
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
});

GridView.displayName = 'GridView';

interface FileCardProps {
  doc: DocumentMetadata;
  isSelected: boolean;
  onClick: (_doc: DocumentMetadata, _event: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onContextMenu?: (_event: React.MouseEvent, _doc: DocumentMetadata) => void;
}

const FileCard = memo(({ doc, isSelected, onClick, onDoubleClick, onContextMenu }: FileCardProps) => (
  <div
    role="button"
    tabIndex={0}
    aria-selected={isSelected}
    onClick={(event) => onClick(doc, event)}
    onDoubleClick={onDoubleClick}
    onContextMenu={(event) => {
      event.preventDefault();
      onContextMenu?.(event, doc);
    }}
    onKeyDown={(event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        onDoubleClick();
      }
    }}
    className={`group flex h-[100px] cursor-pointer flex-col justify-between rounded-md border p-3 transition-colors duration-fast ${
      isSelected
        ? 'border-border-default bg-surface-raised'
        : 'border-border-subtle hover:bg-surface'
    }`}
  >
    <FileIcon file={doc} size={18} className="shrink-0" />
    <div className="min-w-0">
      <p className="truncate text-sm text-text-primary" title={doc.fileName}>
        {doc.fileName}
      </p>
      <p className="truncate text-xs tabular-nums text-text-muted">{metaLine(doc)}</p>
    </div>
  </div>
));

FileCard.displayName = 'FileCard';
