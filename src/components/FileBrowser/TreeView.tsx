/**
 * Tree view — user folders plus readable groups for managed imports and web pages.
 * Same hairline rows as the list; folders carry a chevron and a count.
 */

import { useCallback, useMemo, useRef, useState } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronDown, ChevronRight, Folder } from 'lucide-react';

import { CorpusRow, ROW_HEIGHT } from './CorpusRow';
import { isWebDocument } from './docMeta';
import { CorpusEmptyState, ErrorState, FilterEmptyState } from './EmptyStates';
import { useLibraryDocumentsQuery } from '../../hooks/queries/useLibraryDocumentsQuery';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';
import { type DocumentMetadata } from '../../types/fileBrowser';
import { FileBrowserSkeleton } from '../Skeleton';

interface TreeViewProps {
  onFileOpen?: (_doc: DocumentMetadata) => void;
  onContextMenu?: (_event: React.MouseEvent, _doc: DocumentMetadata) => void;
  onRename?: (_doc: DocumentMetadata) => void;
  onDelete?: (_doc: DocumentMetadata) => void;
  onAddFolder?: () => void;
}

const FOLDER_ROW_HEIGHT = 34;
const INDENT_PX = 14;

interface FolderNode {
  path: string;
  name: string;
  folders: Map<string, FolderNode>;
  docs: DocumentMetadata[];
  count: number;
}

type TreeRow =
  | { kind: 'folder'; node: FolderNode; depth: number }
  | { kind: 'doc'; doc: DocumentMetadata; depth: number };

const makeFolder = (path: string, name: string): FolderNode => ({
  path,
  name,
  folders: new Map(),
  docs: [],
  count: 0,
});

const directorySegments = (doc: DocumentMetadata): string[] => {
  const segments = doc.filePath.replace(/\\/g, '/').split('/').filter(Boolean);
  return segments.slice(0, -1);
};

const documentGroup = (doc: DocumentMetadata): { path: string; name: string } | null => {
  // Virtual roots use keys that cannot collide with real filesystem folders.
  if (isWebDocument(doc)) return { path: '\0web', name: 'Web' };

  // ContentAddressedStorage keeps imports at .lattice/files/{sha256}/{filename}.
  // Recognize that layout specifically; hash-named user folders are still folders.
  const path = doc.filePath.replace(/\\/g, '/');
  if (/(?:^|\/)\.lattice\/files\/[a-f0-9]{64}\/[^/]+$/i.test(path)) {
    return { path: '\0imports', name: 'Imported files' };
  }
  return null;
};

/** Collapse chains of single-child folders so the tree starts where it branches. */
const compress = (node: FolderNode): FolderNode => {
  let current = node;
  while (current.docs.length === 0 && current.folders.size === 1) {
    const [only] = Array.from(current.folders.values());
    current = { ...only, name: `${current.name}/${only.name}` };
  }
  const folders = new Map<string, FolderNode>();
  for (const [key, child] of current.folders) {
    folders.set(key, compress(child));
  }
  return { ...current, folders };
};

const buildTree = (documents: DocumentMetadata[]): FolderNode[] => {
  const roots = new Map<string, FolderNode>();

  for (const doc of documents) {
    const group = documentGroup(doc);
    if (group) {
      const node = roots.get(group.path) ?? makeFolder(group.path, group.name);
      node.docs.push(doc);
      node.count += 1;
      roots.set(group.path, node);
      continue;
    }

    const segments = directorySegments(doc);
    if (segments.length === 0) {
      const orphan = roots.get('') ?? makeFolder('', 'Ungrouped');
      orphan.docs.push(doc);
      orphan.count += 1;
      roots.set('', orphan);
      continue;
    }

    let level = roots;
    let path = '';
    let node: FolderNode | undefined;
    for (const segment of segments) {
      path = path ? `${path}/${segment}` : segment;
      node = level.get(segment);
      if (!node) {
        node = makeFolder(path, segment);
        level.set(segment, node);
      }
      node.count += 1;
      level = node.folders;
    }
    node?.docs.push(doc);
  }

  return Array.from(roots.values())
    .map((root) => compress(root))
    .sort((a, b) => a.name.localeCompare(b.name));
};

const flatten = (
  nodes: FolderNode[],
  expanded: Set<string>,
  depth: number,
  rows: TreeRow[]
): void => {
  for (const node of nodes) {
    rows.push({ kind: 'folder', node, depth });
    if (!expanded.has(node.path)) {
      continue;
    }
    const children = Array.from(node.folders.values()).sort((a, b) => a.name.localeCompare(b.name));
    flatten(children, expanded, depth + 1, rows);
    for (const doc of node.docs) {
      rows.push({ kind: 'doc', doc, depth: depth + 1 });
    }
  }
};

export const TreeView = ({
  onFileOpen,
  onContextMenu,
  onRename,
  onDelete,
  onAddFolder,
}: TreeViewProps) => {
  const { documents, filteredDocuments, isLoading, error } = useLibraryDocumentsQuery();
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const selectFile = useFileBrowserStore(state => state.selectFile);
  const toggleSelection = useFileBrowserStore(state => state.toggleSelection);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const setFocusedDocument = useFileBrowserStore(state => state.setFocusedDocument);

  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const parentRef = useRef<HTMLDivElement>(null);

  const tree = useMemo(() => buildTree(filteredDocuments), [filteredDocuments]);

  const rows = useMemo(() => {
    const allPaths = new Set<string>();
    const walk = (nodes: FolderNode[]) => {
      for (const node of nodes) {
        allPaths.add(node.path);
        walk(Array.from(node.folders.values()));
      }
    };
    walk(tree);

    const expanded = new Set<string>();
    for (const path of allPaths) {
      if (!collapsed.has(path)) {
        expanded.add(path);
      }
    }

    const result: TreeRow[] = [];
    flatten(tree, expanded, 0, result);
    return result;
  }, [collapsed, tree]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => (rows[index]?.kind === 'folder' ? FOLDER_ROW_HEIGHT : ROW_HEIGHT),
    overscan: 10,
  });

  const toggleFolder = useCallback((path: string) => {
    setCollapsed((previous) => {
      const next = new Set(previous);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const handleDocClick = useCallback(
    (doc: DocumentMetadata, event: React.MouseEvent) => {
      setFocusedDocument(doc.id);
      if (event.metaKey || event.ctrlKey) {
        toggleSelection(doc.id);
        return;
      }
      clearSelection();
      selectFile(doc.id);
    },
    [clearSelection, selectFile, toggleSelection, setFocusedDocument]
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

  if (rows.length === 0) {
    return <FilterEmptyState />;
  }

  const selectionActive = selectedDocumentIds.size > 0;

  return (
    <div ref={parentRef} className="h-full overflow-y-auto border-t border-border-subtle">
      <div style={{ height: `${virtualizer.getTotalSize()}px`, position: 'relative' }}>
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const row = rows[virtualRow.index];
          if (!row) return null;

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
              {row.kind === 'folder' ? (
                <button
                  type="button"
                  onClick={() => toggleFolder(row.node.path)}
                  aria-expanded={!collapsed.has(row.node.path)}
                  className="row-hover flex w-full items-center gap-2 rounded-lg pr-3 text-left"
                  style={{
                    height: FOLDER_ROW_HEIGHT,
                    paddingLeft: 12 + row.depth * INDENT_PX,
                  }}
                >
                  {collapsed.has(row.node.path) ? (
                    <ChevronRight className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.75} />
                  ) : (
                    <ChevronDown className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.75} />
                  )}
                  <Folder className="h-4 w-4 shrink-0 text-text-muted" strokeWidth={1.75} />
                  <span className="truncate text-sm text-text-primary">{row.node.name}</span>
                  <span className="shrink-0 text-xs tabular-nums text-text-muted">
                    {row.node.count.toLocaleString()}
                  </span>
                </button>
              ) : (
                <CorpusRow
                  doc={row.doc}
                  indent={row.depth * INDENT_PX}
                  isSelected={selectedDocumentIds.has(row.doc.id)}
                  selectionActive={selectionActive}
                  onClick={(event) => handleDocClick(row.doc, event)}
                  onDoubleClick={() => onFileOpen?.(row.doc)}
                  onToggleSelect={() => toggleSelection(row.doc.id)}
                  onContextMenu={(event) => onContextMenu?.(event, row.doc)}
                  onRename={onRename ? () => onRename(row.doc) : undefined}
                  onDelete={onDelete ? () => onDelete(row.doc) : undefined}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};
