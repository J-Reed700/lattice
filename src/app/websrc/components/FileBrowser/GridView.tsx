/**
 * Grid View Component
 *
 * Purpose: Card-based grid layout with file thumbnails
 *
 * Features:
 * - Thumbnail previews for images
 * - File cards with metadata
 * - Responsive grid layout
 * - Multi-select
 * - Context menu
 */

import { useCallback, memo, useRef } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';
import { Check } from 'lucide-react';

import { type DocumentMetadata, formatFileDate } from '../../types/fileBrowser';
import { FileBrowserSkeleton } from '../Skeleton';
import { FileIcon } from './FileIcon';
import { FileTypeBadge } from './FileTypeBadge';
import { useFileBrowserStore, selectFilteredDocuments } from '../../stores/fileBrowserStore';

interface GridViewProps {
  onFileOpen?: (_doc: DocumentMetadata) => void;
  onContextMenu?: (_event: React.MouseEvent, _doc: DocumentMetadata) => void;
}

export const GridView = memo(({ onFileOpen, onContextMenu }: GridViewProps) => {
  const documents = useFileBrowserStore(selectFilteredDocuments) || [];
  const allDocuments = useFileBrowserStore(state => state.documents) || [];
  const searchQuery = useFileBrowserStore(state => state.searchQuery);
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const selectFile = useFileBrowserStore(state => state.selectFile);
  const toggleSelection = useFileBrowserStore(state => state.toggleSelection);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const isLoading = useFileBrowserStore(state => state.isLoading);
  const error = useFileBrowserStore(state => state.error);

  const parentRef = useRef<HTMLDivElement>(null);

  // Calculate number of columns based on container width
  // This is a simplified version - in production you'd want to match Tailwind breakpoints
  const COLUMNS = 6; // Matches xl:grid-cols-6
  const rowCount = Math.ceil(documents.length / COLUMNS);

  // Virtual scrolling setup for rows
  const rowVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 280, // Estimated row height (card height + gap)
    overscan: 2, // Number of rows to render outside visible area
  });

  const handleCardClick = useCallback(
    (doc: DocumentMetadata, event: React.MouseEvent) => {
      if (event.metaKey || event.ctrlKey) {
        toggleSelection(doc.id);
      } else {
        clearSelection();
        selectFile(doc.id);
      }
    },
    [toggleSelection, selectFile, clearSelection]
  );

  const handleCardDoubleClick = useCallback(
    (doc: DocumentMetadata) => {
      onFileOpen?.(doc);
    },
    [onFileOpen]
  );

  if (isLoading) {
    return <FileBrowserSkeleton view="grid" count={15} />;
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center text-[var(--error)]">
          <p className="font-semibold mb-1">Error loading files</p>
          <p className="text-sm">{error}</p>
        </div>
      </div>
    );
  }

  if (allDocuments.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center text-[var(--text-secondary)]">
          <p className="font-semibold mb-1">No documents found</p>
          <p className="text-sm">Add files to start indexing</p>
        </div>
      </div>
    );
  }

  if (documents.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center text-[var(--text-secondary)] max-w-md px-6">
          <p className="font-semibold mb-1">No matches found</p>
          <p className="text-sm">
            {searchQuery.trim()
              ? `No files matched "${searchQuery.trim()}" by name or content.`
              : 'Try changing filters to see more files.'}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div ref={parentRef} className="overflow-auto h-full p-4 bg-[linear-gradient(180deg,var(--surface-elevated),var(--bg-secondary))]">
      <div
        style={{
          height: `${rowVirtualizer.getTotalSize()}px`,
          position: 'relative',
        }}
      >
        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
          const startIndex = virtualRow.index * COLUMNS;
          const endIndex = Math.min(startIndex + COLUMNS, documents.length);
          const rowDocs = documents.slice(startIndex, endIndex);

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
              <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
                {rowDocs.map((doc) => (
                  <FileCard
                    key={doc.id}
                    doc={doc}
                    isSelected={selectedDocumentIds.has(doc.id)}
                    onClick={handleCardClick}
                    onDoubleClick={handleCardDoubleClick}
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

interface FileCardProps {
  doc: DocumentMetadata;
  isSelected: boolean;
  onClick: (_doc: DocumentMetadata, _event: React.MouseEvent) => void;
  onDoubleClick: (_doc: DocumentMetadata) => void;
  onContextMenu?: (_event: React.MouseEvent, _doc: DocumentMetadata) => void;
}

const FileCard = memo(({
  doc,
  isSelected,
  onClick,
  onDoubleClick,
  onContextMenu,
}: FileCardProps) => {
  const normalizedType = (doc.fileType ?? '').toLowerCase();
  const isImage = ['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp'].includes(normalizedType);

  const handleContextMenu = useCallback(
    (event: React.MouseEvent) => {
      event.preventDefault();
      onContextMenu?.(event, doc);
    },
    [onContextMenu, doc]
  );

  return (
    <div
      className={`relative group cursor-pointer overflow-hidden rounded-xl border transition-all duration-200 ${
        isSelected
          ? 'border-[var(--accent-primary)] bg-[var(--accent-light)]/30 shadow-[var(--shadow-md)]'
          : 'border-[var(--border-color)] bg-[var(--surface-elevated)] hover:-translate-y-0.5 hover:border-[var(--accent-primary)]/35 hover:shadow-[var(--shadow-lg)]'
      }`}
      onClick={(e) => onClick(doc, e)}
      onDoubleClick={() => onDoubleClick(doc)}
      onContextMenu={handleContextMenu}
    >
      {/* Selection indicator */}
      {isSelected && (
        <div className="absolute right-2 top-2 z-10 flex h-6 w-6 items-center justify-center rounded-full bg-[var(--accent-primary)] shadow-sm">
          <Check className="w-4 h-4 text-white" />
        </div>
      )}

      {/* Thumbnail/Icon area */}
      <div className="aspect-square flex items-center justify-center bg-gradient-to-br from-[var(--bg-secondary)] to-[var(--surface-hover)] p-4">
        {isImage ? (
          <div className="w-full h-full flex items-center justify-center">
            <img
              src={`file://${doc.filePath}`}
              alt={doc.fileName}
              className="max-w-full max-h-full object-contain"
              loading="lazy"
              onError={(e) => {
                // Fallback to icon if image fails to load
                const target = e.target as HTMLImageElement;
                target.style.display = 'none';
                const icon = target.parentElement?.querySelector('.fallback-icon');
                if (icon) {
                  (icon as HTMLElement).style.display = 'block';
                }
              }}
            />
            <div className="fallback-icon hidden">
              <FileIcon file={doc} size={48} />
            </div>
          </div>
        ) : (
          <FileIcon file={doc} size={48} />
        )}
      </div>

      {/* File info */}
      <div className="space-y-1.5 p-3">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium truncate flex-1 text-[var(--text-primary)]" title={doc.fileName}>
            {doc.fileName}
          </h3>
          <span
            className="flex-shrink-0 w-2 h-2 rounded-full bg-[var(--success)]"
            title="Indexed"
          />
        </div>

        <div className="flex items-center justify-between text-xs text-[var(--text-secondary)]">
          <span>{doc.wordCount ? `${doc.wordCount} words` : '-'}</span>
          <span>{formatFileDate(doc.modifiedAt)}</span>
        </div>

        {/* FileTypeBadge for visual file type identification */}
        <div className="flex justify-start mt-2">
          <FileTypeBadge file={doc} size="sm" />
        </div>
      </div>
    </div>
  );
}, (prevProps, nextProps) => 
  // Only re-render if doc id or selection state changes
   (
    prevProps.doc.id === nextProps.doc.id &&
    prevProps.isSelected === nextProps.isSelected &&
    prevProps.doc.modifiedAt === nextProps.doc.modifiedAt
  )
);
