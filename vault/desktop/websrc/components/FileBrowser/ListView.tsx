/**
 * List View Component
 *
 * Purpose: Table-based file list with sortable columns
 *
 * Features:
 * - Sortable columns (name, size, date, type)
 * - Multi-select with checkboxes
 * - Context menu on right-click
 * - Keyboard navigation
 * - Responsive column widths
 */

import { useState, useCallback, memo, useRef, useMemo } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowUp, ArrowDown, Edit3, Trash2 } from 'lucide-react';

import { type DocumentMetadata, type SortField, type SortOrder } from '../../types/fileBrowser';
import { FileBrowserSkeleton } from '../Skeleton';
import { FileIcon } from './FileIcon';
import { FileTypeBadge } from './FileTypeBadge';
import { useFileBrowserStore, selectFilteredDocuments, selectDensity } from '../../stores/fileBrowserStore';
import { formatRelativeTime, getDateGroup, groupBy, type DateGroup } from '../../utils/dateUtils';
import { Checkbox } from '../ui/Checkbox';

interface ListViewProps {
  onFileOpen?: (doc: DocumentMetadata) => void;
  onContextMenu?: (event: React.MouseEvent, doc: DocumentMetadata) => void;
  onRename?: (doc: DocumentMetadata) => void;
  onDelete?: (doc: DocumentMetadata) => void;
}

const DENSITY_HEIGHTS = {
  compact: 40,
  comfortable: 50,
  spacious: 64,
} as const;

const HEADER_HEIGHT = 32;

type ListItem = { type: 'header'; label: string } | { type: 'doc'; data: DocumentMetadata };

export const ListView = memo(({ onFileOpen, onContextMenu, onRename, onDelete }: ListViewProps) => {
  const filteredDocuments = useFileBrowserStore(selectFilteredDocuments);
  const groupByDate = useFileBrowserStore(state => state.groupByDate);
  const density = useFileBrowserStore(selectDensity);
  const documents = useFileBrowserStore(state => state.documents);
  const searchQuery = useFileBrowserStore(state => state.searchQuery);
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const listColumns = useFileBrowserStore(state => state.listColumns) ?? {
    words: true,
    modified: true,
    type: true,
  };
  const sortField = useFileBrowserStore(state => state.sortField);
  const sortOrder = useFileBrowserStore(state => state.sortOrder);
  const setSortField = useFileBrowserStore(state => state.setSortField);
  const toggleSortOrder = useFileBrowserStore(state => state.toggleSortOrder);
  const selectFile = useFileBrowserStore(state => state.selectFile);
  const toggleSelection = useFileBrowserStore(state => state.toggleSelection);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const selectAll = useFileBrowserStore(state => state.selectAll);
  const isLoading = useFileBrowserStore(state => state.isLoading);
  const error = useFileBrowserStore(state => state.error);

  // Memoize grouped documents to prevent infinite re-renders
  const items = useMemo((): ListItem[] => {
    const result: ListItem[] = [];

    if (groupByDate) {
      const grouped = groupBy(filteredDocuments, doc => getDateGroup(doc.modifiedAt));
      const groupOrder: DateGroup[] = ['Today', 'Yesterday', 'This Week', 'Last Week', 'This Month', 'Older'];

      for (const label of groupOrder) {
        const docs = grouped[label];
        if (docs && docs.length > 0) {
          result.push({ type: 'header', label });
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
  const shouldVirtualize = items.length > 60;

  // Virtual scrolling setup
  const rowHeight = DENSITY_HEIGHTS[density];
  const rowVirtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => {
      const item = items[index];
      return item?.type === 'header' ? HEADER_HEIGHT : rowHeight;
    },
    overscan: 10,
  });

  const handleHeaderClick = useCallback(
    (field: SortField) => {
      if (field === sortField) {
        toggleSortOrder();
      } else {
        setSortField(field);
      }
    },
    [sortField, setSortField, toggleSortOrder]
  );

  const handleRowClick = useCallback(
    (doc: DocumentMetadata, _itemIndex: number, event: React.MouseEvent) => {
      // Find all doc items for range selection
      const docItems = items.filter((item): item is Extract<ListItem, { type: 'doc' }> => item.type === 'doc');
      const docIndex = docItems.findIndex(item => item.data.id === doc.id);

      if (event.metaKey || event.ctrlKey) {
        // Multi-select
        toggleSelection(doc.id);
        setLastSelectedIndex(docIndex);
      } else if (event.shiftKey && lastSelectedIndex !== -1) {
        // Range select
        clearSelection();
        const start = Math.min(lastSelectedIndex, docIndex);
        const end = Math.max(lastSelectedIndex, docIndex);
        docItems.slice(start, end + 1).forEach((item) => selectFile(item.data.id));
      } else {
        // Single select
        clearSelection();
        selectFile(doc.id);
        setLastSelectedIndex(docIndex);
      }
    },
    [toggleSelection, selectFile, clearSelection, items, lastSelectedIndex]
  );

  const handleRowDoubleClick = useCallback(
    (doc: DocumentMetadata) => {
      onFileOpen?.(doc);
    },
    [onFileOpen]
  );

  const handleCheckboxChange = useCallback(
    (doc: DocumentMetadata) => {
      toggleSelection(doc.id);
    },
    [toggleSelection]
  );

  const handleSelectAll = useCallback(() => {
    if (selectedDocumentIds.size === documents.length) {
      clearSelection();
    } else {
      selectAll();
    }
  }, [selectedDocumentIds.size, documents.length, clearSelection, selectAll]);

  const allSelected = selectedDocumentIds.size === documents.length && documents.length > 0;
  const someSelected = selectedDocumentIds.size > 0 && selectedDocumentIds.size < documents.length;
  const tableColumnCount =
    3 +
    (listColumns.words ? 1 : 0) +
    (listColumns.modified ? 1 : 0) +
    (listColumns.type ? 1 : 0);

  if (isLoading) {
    return <FileBrowserSkeleton view="list" count={12} />;
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

  if (documents.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center text-[var(--text-secondary)]">
          <p className="font-semibold mb-1">No documents found</p>
          <p className="text-sm">Add files to start indexing</p>
        </div>
      </div>
    );
  }

  if (items.length === 0) {
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

  const renderDocumentRow = (doc: DocumentMetadata, key: string | number) => (
    <tr
      key={key}
      className={`group border-b border-[var(--border-color)] cursor-pointer transition-colors ${
        selectedDocumentIds.has(doc.id)
          ? 'bg-[var(--accent-light)]/35'
          : 'hover:bg-[var(--surface-hover)]/90'
      }`}
      onClick={(e) => handleRowClick(doc, 0, e)}
      onDoubleClick={() => handleRowDoubleClick(doc)}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu?.(e, doc);
      }}
    >
      <td className="w-12 px-4 py-3" onClick={(e) => e.stopPropagation()}>
        <Checkbox
          checked={selectedDocumentIds.has(doc.id)}
          onChange={() => handleCheckboxChange(doc)}
          aria-label={`Select ${doc.fileName}`}
        />
      </td>
      <td className="px-4 py-3">
        <div className="flex items-center gap-3">
          <FileIcon file={doc} size={20} />
          <span className="text-sm font-medium truncate">{doc.fileName}</span>
          <span
            className="flex-shrink-0 w-2 h-2 rounded-full bg-[var(--success-light)]"
            title="Indexed"
          />
        </div>
      </td>
      {listColumns.words && (
        <td className="w-32 px-4 py-3 text-sm text-[var(--text-secondary)]">
          {doc.wordCount ? `${doc.wordCount} words` : '-'}
        </td>
      )}
      {listColumns.modified && (
        <td className="w-40 px-4 py-3 text-sm text-[var(--text-secondary)]">
          <span title={new Date(doc.modifiedAt).toLocaleString()}>
            {formatRelativeTime(doc.modifiedAt)}
          </span>
        </td>
      )}
      {listColumns.type && (
        <td className="w-32 px-4 py-3">
          <FileTypeBadge file={doc} size="sm" />
        </td>
      )}
      <td className="w-24 px-4 py-3">
        <div className="opacity-0 group-hover:opacity-100 transition-opacity flex gap-1 justify-end">
          {onRename && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onRename(doc);
              }}
              className="p-1.5 hover:bg-[var(--surface-elevated)] rounded-lg transition-colors"
              aria-label="Rename"
              title="Rename"
            >
              <Edit3 className="w-4 h-4" />
            </button>
          )}
          {onDelete && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onDelete(doc);
              }}
              className="p-1.5 hover:bg-[var(--surface-elevated)] rounded-lg transition-colors text-[var(--error)]"
              aria-label="Delete"
              title="Delete"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          )}
        </div>
      </td>
    </tr>
  );

  return (
    <div ref={parentRef} className="overflow-auto h-full bg-[linear-gradient(180deg,var(--surface-elevated),var(--bg-secondary))]">
      <table className="w-full border-collapse">
        <thead className="sticky top-0 z-20 border-b border-[var(--border-color)] bg-[var(--surface-elevated)]/95 backdrop-blur-sm">
          <tr>
            <th className="w-12 px-4 py-3 text-left">
              <Checkbox
                checked={allSelected}
                indeterminate={someSelected}
                onChange={handleSelectAll}
                aria-label="Select all files"
              />
            </th>
            <ColumnHeader
              label="Name"
              field="name"
              currentField={sortField}
              order={sortOrder}
              onClick={handleHeaderClick}
            />
            {listColumns.words && (
              <ColumnHeader
                label="Words"
                field="size"
                currentField={sortField}
                order={sortOrder}
                onClick={handleHeaderClick}
                className="w-32"
              />
            )}
            {listColumns.modified && (
              <ColumnHeader
                label="Modified"
                field="modified"
                currentField={sortField}
                order={sortOrder}
                onClick={handleHeaderClick}
                className="w-40"
              />
            )}
            {listColumns.type && (
              <ColumnHeader
                label="Type"
                field="type"
                currentField={sortField}
                order={sortOrder}
                onClick={handleHeaderClick}
                className="w-32"
              />
            )}
            <th className="w-24 px-4 py-3 text-left">
              <span className="text-sm font-semibold text-[var(--text-secondary)]">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {shouldVirtualize ? (
            <tr style={{ height: `${rowVirtualizer.getTotalSize()}px` }}>
              <td colSpan={tableColumnCount} style={{ padding: 0, border: 'none' }}>
                <div style={{ position: 'relative' }}>
                  {rowVirtualizer.getVirtualItems().map((virtualRow) => {
                    const item = items[virtualRow.index];

                    if (item.type === 'header') {
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
                          className="z-10 border-b border-[var(--border-color)] bg-[var(--accent-light)]/20 px-4 py-1.5"
                        >
                          <h3 className="text-[11px] font-semibold uppercase tracking-widest text-[var(--accent-primary)]">
                            {item.label}
                          </h3>
                        </div>
                      );
                    }

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
                        <table className="w-full border-collapse">
                          <tbody>{renderDocumentRow(item.data, String(virtualRow.key))}</tbody>
                        </table>
                      </div>
                    );
                  })}
                </div>
              </td>
            </tr>
          ) : (
            items.map((item, index) =>
              item.type === 'header' ? (
                <tr
                  key={`header-${item.label}-${index}`}
                  className="border-b border-[var(--border-color)] bg-[var(--accent-light)]/20"
                >
                  <td colSpan={tableColumnCount} className="px-4 py-1.5">
                    <h3 className="text-[11px] font-semibold uppercase tracking-widest text-[var(--accent-primary)]">
                      {item.label}
                    </h3>
                  </td>
                </tr>
              ) : (
                renderDocumentRow(item.data, item.data.id)
              )
            )
          )}
        </tbody>
      </table>
    </div>
  );
});

interface ColumnHeaderProps {
  label: string;
  field: SortField;
  currentField: SortField;
  order: SortOrder;
  onClick: (field: SortField) => void;
  className?: string;
}

function ColumnHeader({
  label,
  field,
  currentField,
  order,
  onClick,
  className = '',
}: ColumnHeaderProps) {
  const isActive = field === currentField;

  return (
    <th className={`px-4 py-3 text-left ${className}`}>
      <button
        onClick={() => onClick(field)}
        className="flex items-center gap-2 text-sm font-semibold text-[var(--text-secondary)] transition-colors hover:text-[var(--text-primary)]"
      >
        {label}
        {isActive && (
          <span className="flex-shrink-0">
            {order === 'asc' ? (
              <ArrowUp className="w-4 h-4" />
            ) : (
              <ArrowDown className="w-4 h-4" />
            )}
          </span>
        )}
      </button>
    </th>
  );
}
