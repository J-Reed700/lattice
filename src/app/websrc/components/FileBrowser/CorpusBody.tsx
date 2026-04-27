import { useCallback, useMemo, useRef, useState } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';

import { CorpusGroupHeader, GROUP_HEADER_HEIGHT_PX } from './CorpusGroupHeader';
import { CorpusRow, ROW_HEIGHTS } from './CorpusRow';
import { FilterEmptyState } from './EmptyStates';
import { FileBrowserSkeleton } from '../Skeleton';
import { type CorpusListItem, type DensityMode } from './hooks/useCorpusBrowser';
import { type DocumentMetadata } from '../../types/fileBrowser';

interface CorpusBodyProps {
  items: CorpusListItem[];
  density: DensityMode;
  selectedIds: Set<string>;
  hasActiveFilters: boolean;
  isLoading: boolean;
  onToggleSelect: (docId: string) => void;
  onRangeSelect: (docId: string) => void;
  onSingleSelect: (docId: string) => void;
  onOpen: (doc: DocumentMetadata) => void;
  onChatWith: (doc: DocumentMetadata) => void;
  onRename: (doc: DocumentMetadata) => void;
  onDelete: (doc: DocumentMetadata) => void;
  onContextMenu: (event: React.MouseEvent, doc: DocumentMetadata) => void;
  onClearFilters: () => void;
}

const OVERSCAN = 8;

/**
 * Virtualized corpus body with group headers — §6.1 + §6.3. Reuses
 * `@tanstack/react-virtual` and mirrors the pattern used by the legacy
 * ListView / TreeView virtualized document tables.
 */
export function CorpusBody({
  items,
  density,
  selectedIds,
  hasActiveFilters,
  isLoading,
  onToggleSelect,
  onRangeSelect,
  onSingleSelect,
  onOpen,
  onChatWith,
  onRename,
  onDelete,
  onContextMenu,
  onClearFilters,
}: CorpusBodyProps) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [lastDocIndex, setLastDocIndex] = useState<number>(-1);

  const rowHeight = ROW_HEIGHTS[density];

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => {
      const item = items[index];
      return item?.type === 'header' ? GROUP_HEADER_HEIGHT_PX : rowHeight;
    },
    overscan: OVERSCAN,
    getItemKey: (index) => {
      const item = items[index];
      if (!item) return index;
      if (item.type === 'header') return `h:${item.groupKey}`;
      return `d:${item.doc.id}`;
    },
  });

  const selectionActive = selectedIds.size > 0;

  const docIndexById = useMemo(() => {
    const map = new Map<string, number>();
    let docPosition = 0;
    for (const item of items) {
      if (item.type === 'doc') {
        map.set(item.doc.id, docPosition);
        docPosition += 1;
      }
    }
    return map;
  }, [items]);

  const handleRowInteraction = useCallback(
    (doc: DocumentMetadata, mode: 'toggle' | 'range' | 'single') => {
      const docIndex = docIndexById.get(doc.id) ?? -1;
      if (mode === 'toggle') {
        onToggleSelect(doc.id);
        setLastDocIndex(docIndex);
      } else if (mode === 'range') {
        if (lastDocIndex >= 0 && docIndex >= 0) {
          const start = Math.min(lastDocIndex, docIndex);
          const end = Math.max(lastDocIndex, docIndex);
          const docIds: string[] = [];
          for (const item of items) {
            if (item.type === 'doc') {
              docIds.push(item.doc.id);
            }
          }
          for (let index = start; index <= end; index += 1) {
            const id = docIds[index];
            if (id) {
              onRangeSelect(id);
            }
          }
        } else {
          onToggleSelect(doc.id);
          setLastDocIndex(docIndex);
        }
      } else {
        onSingleSelect(doc.id);
        setLastDocIndex(docIndex);
      }
    },
    [docIndexById, items, lastDocIndex, onRangeSelect, onSingleSelect, onToggleSelect],
  );

  if (isLoading) {
    return <FileBrowserSkeleton view="list" count={12} />;
  }

  if (items.length === 0) {
    if (hasActiveFilters) {
      return <FilterEmptyState onClearFilters={onClearFilters} />;
    }
    return (
      <div className="flex flex-col items-center justify-center gap-2 px-6 py-16 text-center">
        <p className="text-sm text-[hsl(var(--text-tertiary))]">No documents yet.</p>
      </div>
    );
  }

  const totalSize = virtualizer.getTotalSize();

  return (
    <div
      ref={scrollRef}
      className="relative h-full w-full overflow-y-auto"
      role="region"
      aria-label="Corpus documents"
    >
      <div
        style={{ height: totalSize, position: 'relative' }}
        className="w-full"
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const item = items[virtualRow.index];
          if (!item) return null;

          return (
            <div
              key={virtualRow.key}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                right: 0,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              {item.type === 'header' ? (
                <CorpusGroupHeader label={item.groupLabel} count={item.groupCount} />
              ) : (
                <CorpusRow
                  doc={item.doc}
                  density={density}
                  isSelected={selectedIds.has(item.doc.id)}
                  selectionActive={selectionActive}
                  onToggleSelect={(mode) => handleRowInteraction(item.doc, mode)}
                  onOpen={() => onOpen(item.doc)}
                  onChatWith={() => onChatWith(item.doc)}
                  onRename={() => onRename(item.doc)}
                  onDelete={() => onDelete(item.doc)}
                  onContextMenu={(event) => onContextMenu(event, item.doc)}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
