/**
 * List view — hairline rows, no bounding card, no column chrome.
 * Sorting lives in the toolbar; the row carries name plus one muted meta line.
 */

import { useState, useCallback, memo, useRef, useMemo } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';

import { CorpusRow, ROW_HEIGHT } from './CorpusRow';
import { CorpusEmptyState, ErrorState, FilterEmptyState } from './EmptyStates';
import { useLibraryDocumentsQuery } from '../../hooks/queries/useLibraryDocumentsQuery';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';
import { type DocumentMetadata } from '../../types/fileBrowser';
import { getDateGroup, groupBy, type DateGroup } from '../../utils/dateUtils';
import { FileBrowserSkeleton } from '../Skeleton';

interface ListViewProps {
  onFileOpen?: (_doc: DocumentMetadata) => void;
  onContextMenu?: (_event: React.MouseEvent, _doc: DocumentMetadata) => void;
  onRename?: (_doc: DocumentMetadata) => void;
  onDelete?: (_doc: DocumentMetadata) => void;
  onAddFolder?: () => void;
}

const HEADER_HEIGHT = 30;

const GROUP_ORDER: DateGroup[] = [
  'Today',
  'Yesterday',
  'This Week',
  'Last Week',
  'This Month',
  'Older',
];

const GROUP_LABELS: Record<DateGroup, string> = {
  Today: 'Today',
  Yesterday: 'Yesterday',
  'This Week': 'This week',
  'Last Week': 'Last week',
  'This Month': 'This month',
  Older: 'Older',
};

type ListItem = { type: 'header'; label: string } | { type: 'doc'; data: DocumentMetadata };

export const ListView = memo(({
  onFileOpen,
  onContextMenu,
  onRename,
  onDelete,
  onAddFolder,
}: ListViewProps) => {
  const { documents, filteredDocuments, isLoading, error } = useLibraryDocumentsQuery();
  const groupByDate = useFileBrowserStore(state => state.groupByDate);
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const selectFile = useFileBrowserStore(state => state.selectFile);
  const toggleSelection = useFileBrowserStore(state => state.toggleSelection);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const setFocusedDocument = useFileBrowserStore(state => state.setFocusedDocument);

  const items = useMemo((): ListItem[] => {
    const result: ListItem[] = [];

    if (groupByDate) {
      const grouped = groupBy(filteredDocuments, doc => getDateGroup(doc.modifiedAt));

      for (const label of GROUP_ORDER) {
        const docs = grouped[label];
        if (docs && docs.length > 0) {
          result.push({ type: 'header', label: GROUP_LABELS[label] });
          docs.forEach(doc => result.push({ type: 'doc', data: doc }));
        }
      }

      return result;
    }

    filteredDocuments.forEach(doc => result.push({ type: 'doc', data: doc }));
    return result;
  }, [filteredDocuments, groupByDate]);

  const [lastSelectedIndex, setLastSelectedIndex] = useState<number>(-1);
  const parentRef = useRef<HTMLDivElement>(null);

  const rowVirtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => (items[index]?.type === 'header' ? HEADER_HEIGHT : ROW_HEIGHT),
    overscan: 10,
  });

  const handleRowClick = useCallback(
    (doc: DocumentMetadata, event: React.MouseEvent) => {
      const docItems = items.filter(
        (item): item is Extract<ListItem, { type: 'doc' }> => item.type === 'doc'
      );
      const docIndex = docItems.findIndex(item => item.data.id === doc.id);

      // The last row clicked is what the neighborhood panel is about, on every
      // branch — including the modifier branches.
      setFocusedDocument(doc.id);

      if (event.metaKey || event.ctrlKey) {
        toggleSelection(doc.id);
        setLastSelectedIndex(docIndex);
      } else if (event.shiftKey && lastSelectedIndex !== -1) {
        clearSelection();
        const start = Math.min(lastSelectedIndex, docIndex);
        const end = Math.max(lastSelectedIndex, docIndex);
        docItems.slice(start, end + 1).forEach((item) => selectFile(item.data.id));
      } else {
        clearSelection();
        selectFile(doc.id);
        setLastSelectedIndex(docIndex);
      }
    },
    [toggleSelection, selectFile, clearSelection, items, lastSelectedIndex, setFocusedDocument]
  );

  if (isLoading) {
    return <FileBrowserSkeleton view="list" count={12} />;
  }

  if (error) {
    return <ErrorState message={error} />;
  }

  if (documents.length === 0) {
    return <CorpusEmptyState onAddFolder={onAddFolder ?? (() => {})} />;
  }

  if (items.length === 0) {
    return <FilterEmptyState />;
  }

  const selectionActive = selectedDocumentIds.size > 0;

  return (
    <div ref={parentRef} className="h-full overflow-y-auto border-t border-border-subtle">
      <div style={{ height: `${rowVirtualizer.getTotalSize()}px`, position: 'relative' }}>
        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
          const item = items[virtualRow.index];
          if (!item) return null;

          return (
            <div
              key={virtualRow.key}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              {item.type === 'header' ? (
                <div
                  className="flex items-end border-b border-border-subtle px-3 pb-1"
                  style={{ height: HEADER_HEIGHT }}
                >
                  <span className="text-xs font-medium text-text-secondary">{item.label}</span>
                </div>
              ) : (
                <CorpusRow
                  doc={item.data}
                  isSelected={selectedDocumentIds.has(item.data.id)}
                  selectionActive={selectionActive}
                  onClick={(event) => handleRowClick(item.data, event)}
                  onDoubleClick={() => onFileOpen?.(item.data)}
                  onToggleSelect={() => toggleSelection(item.data.id)}
                  onContextMenu={(event) => onContextMenu?.(event, item.data)}
                  onRename={onRename ? () => onRename(item.data) : undefined}
                  onDelete={onDelete ? () => onDelete(item.data) : undefined}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
});

ListView.displayName = 'ListView';
