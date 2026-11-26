/**
 * Grid view — tiles with a hairline border, a name, and one muted meta line.
 * Virtualized rows dynamically match the observed container column count so
 * cards never wrap, misalign, or clip across viewport or rail widths.
 */

import { useCallback, useEffect, useState, memo, useRef } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';
import { MoreHorizontal } from 'lucide-react';

import { FileBrowserSkeleton } from '@/components/Skeleton';
import { useLibraryDocumentsQuery } from '@/features/files/hooks/useLibraryDocumentsQuery';
import { formatDisplayPath, metaLine } from '@/features/files/model/documentMetadata';
import { useFileBrowserStore } from '@/features/files/stores/fileBrowserStore';
import { type DocumentMetadata } from '@/types/fileBrowser';

import { CorpusEmptyState, ErrorState, FilterEmptyState } from './EmptyStates';
import { FileIcon } from './FileIcon';

interface GridViewProps {
  onFileOpen?: (_doc: DocumentMetadata) => void;
  onContextMenu?: (_event: React.MouseEvent, _doc: DocumentMetadata) => void;
  onAddFolder?: () => void;
}

const DEFAULT_COLUMNS = 3;
const ROW_HEIGHT = 116;

/**
 * Pure responsive column calculation shared by ResizeObserver and layout so
 * the virtualizer row slice count and CSS grid columns always stay in lockstep.
 */
export function computeGridColumns(containerWidth: number): number {
  if (!Number.isFinite(containerWidth) || containerWidth <= 0) {
    return DEFAULT_COLUMNS;
  }
  if (containerWidth < 480) return 1;
  if (containerWidth < 740) return 2;
  if (containerWidth < 1040) return 3;
  return 4;
}

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
  const [columns, setColumns] = useState<number>(DEFAULT_COLUMNS);

  useEffect(() => {
    const node = parentRef.current;
    if (!node) return;

    const updateColumns = (width: number) => {
      const next = computeGridColumns(width);
      setColumns((prev) => (prev === next ? prev : next));
    };

    if (node.clientWidth > 0) {
      updateColumns(node.clientWidth);
    }

    if (typeof ResizeObserver === 'undefined') {
      return;
    }

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      const width = entry.contentRect?.width ?? node.clientWidth;
      if (width > 0) {
        updateColumns(width);
      }
    });
    observer.observe(node);

    return () => observer.disconnect();
  }, [isLoading, error, allDocuments.length, documents.length]);

  const rowCount = Math.ceil(documents.length / columns);

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
    <div
      ref={parentRef}
      data-grid-columns={columns}
      className="h-full overflow-y-auto pt-1"
    >
      <div style={{ height: `${rowVirtualizer.getTotalSize()}px`, position: 'relative' }}>
        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
          const startIndex = virtualRow.index * columns;
          const rowDocs = documents.slice(startIndex, Math.min(startIndex + columns, documents.length));

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
              <div
                className="grid gap-3 px-0.5 py-0.5"
                style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
              >
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

const FileCard = memo(({ doc, isSelected, onClick, onDoubleClick, onContextMenu }: FileCardProps) => {
  const sourceLocation = formatDisplayPath(doc.filePath);

  return (
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
        if (event.target !== event.currentTarget) return;
        if (event.key === 'Enter') {
          event.preventDefault();
          onDoubleClick();
        }
      }}
      className={`group flex h-[104px] cursor-pointer flex-col justify-between rounded-xl border p-3.5 transition-[transform,border-color,background-color,box-shadow] duration-base focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
        isSelected
          ? 'border-border-default bg-surface-raised shadow-sm ring-1 ring-accent/25'
          : 'border-border-subtle bg-surface shadow-sm hover:-translate-y-px hover:border-border-default hover:shadow-md'
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <FileIcon file={doc} size={16} className="shrink-0" />
          {sourceLocation && (
            <span className="truncate text-[11px] text-text-muted" title={doc.filePath}>
              {sourceLocation}
            </span>
          )}
        </div>
        <button
          type="button"
          aria-label={`Actions for ${doc.fileName}`}
          onClick={event => { event.stopPropagation(); onContextMenu?.(event, doc); }}
          onDoubleClick={event => event.stopPropagation()}
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-text-muted opacity-0 transition-[opacity,background-color,color] duration-fast hover:bg-surface-raised hover:text-text-primary focus-visible:opacity-100 group-hover:opacity-100"
        >
          <MoreHorizontal className="h-4 w-4" />
        </button>
      </div>
      <div className="min-w-0">
        <p className="truncate font-sans text-sm font-semibold tracking-[-0.015em] text-text-primary" title={doc.fileName}>
          {doc.fileName}
        </p>
        <p className="mt-0.5 truncate text-xs tabular-nums text-text-muted">{metaLine(doc)}</p>
      </div>
    </div>
  );
});

FileCard.displayName = 'FileCard';
