import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';

import { useQueries } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, File, FileCode, FileJson, FileText, Folder, FolderOpen, Image } from 'lucide-react';

import { useExplorerStore } from '@/features/explorer/stores/explorerStore';
import { VaultAPI } from '@/lib/api';
import type { ExplorerEntryDto, ExplorerListingDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';

const INDENT_PX = 12;
const ROW_HEIGHT = 26;

export const explorerDirKey = (root: string, path: string) => ['explorer', 'dir', root, path] as const;

const CODE_EXTENSIONS = new Set(['rs', 'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'py', 'go', 'java', 'kt', 'swift', 'c', 'h', 'cc', 'cpp', 'hpp', 'cs', 'rb', 'php', 'sh', 'zsh', 'bash', 'toml', 'yaml', 'yml', 'html', 'css', 'scss', 'sql', 'xml', 'lua', 'dart', 'zig', 'ex', 'exs', 'hs', 'scala', 'vue', 'svelte']);
const TEXT_EXTENSIONS = new Set(['md', 'markdown', 'mdx', 'txt', 'rst', 'adoc']);
const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp']);

function FileGlyph({ name }: { name: string }) {
  const extension = name.includes('.') ? name.split('.').pop()!.toLowerCase() : '';
  const Icon = extension === 'json' ? FileJson
    : CODE_EXTENSIONS.has(extension) ? FileCode
      : TEXT_EXTENSIONS.has(extension) ? FileText
        : IMAGE_EXTENSIONS.has(extension) ? Image
          : File;
  return <Icon className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.6} aria-hidden="true" />;
}

interface Row {
  entry: ExplorerEntryDto;
  depth: number;
  parent: string | null;
}

export interface ExplorerTreeProps {
  root: string;
}

async function listDir(root: string, path: string): Promise<ExplorerListingDto> {
  const result = await VaultAPI.explorerListDir(root, path);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

/**
 * The folder as a tree, one directory read at a time.
 *
 * Only open directories are read. The visible rows are flattened once so the
 * keyboard moves through them in the order they are drawn.
 */
export function ExplorerTree({ root }: ExplorerTreeProps) {
  const expanded = useExplorerStore((state) => state.expanded);
  const openPath = useExplorerStore((state) => state.openPath);
  const toggleExpanded = useExplorerStore((state) => state.toggleExpanded);
  const setExpanded = useExplorerStore((state) => state.setExpanded);
  const openFile = useExplorerStore((state) => state.openFile);
  const [focusedPath, setFocusedPath] = useState<string | null>(null);
  // The keyboard cursor is drawn only while the tree has focus.
  const [hasFocus, setHasFocus] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);

  const dirs = useMemo(() => ['', ...[...expanded].sort()], [expanded]);
  const listings = useQueries({
    queries: dirs.map((path) => ({
      queryKey: explorerDirKey(root, path),
      queryFn: () => listDir(root, path),
      staleTime: 10_000,
    })),
  });
  const listingByPath = new Map(dirs.map((path, index) => [path, listings[index]] as const));

  const rows: Row[] = [];
  const walk = (path: string, depth: number, parent: string | null) => {
    for (const entry of listingByPath.get(path)?.data?.entries ?? []) {
      rows.push({ entry, depth, parent });
      if (entry.kind === 'directory' && expanded.has(entry.path)) walk(entry.path, depth + 1, entry.path);
    }
  };
  walk('', 0, null);

  const rootListing = listingByPath.get('');
  const focusedIndex = rows.findIndex((row) => row.entry.path === (focusedPath ?? openPath));

  // Keep the focused row on screen as the keyboard moves it.
  useEffect(() => {
    if (!focusedPath) return;
    listRef.current?.querySelector<HTMLElement>(`[data-path="${CSS.escape(focusedPath)}"]`)?.scrollIntoView({ block: 'nearest' });
  }, [focusedPath]);

  const activate = (row: Row) => {
    setFocusedPath(row.entry.path);
    if (row.entry.kind === 'directory') toggleExpanded(row.entry.path);
    else openFile(row.entry.path);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (rows.length === 0) return;
    const index = focusedIndex < 0 ? 0 : focusedIndex;
    const row = rows[index];
    const focus = (next: number) => setFocusedPath(rows[Math.max(0, Math.min(rows.length - 1, next))].entry.path);
    switch (event.key) {
      case 'ArrowDown': focus(focusedIndex < 0 ? 0 : index + 1); break;
      case 'ArrowUp': focus(index - 1); break;
      case 'Home': focus(0); break;
      case 'End': focus(rows.length - 1); break;
      case 'ArrowRight':
        if (row.entry.kind !== 'directory') return;
        if (!expanded.has(row.entry.path)) setExpanded(row.entry.path, true);
        else focus(index + 1);
        break;
      case 'ArrowLeft':
        if (row.entry.kind === 'directory' && expanded.has(row.entry.path)) setExpanded(row.entry.path, false);
        else if (row.parent !== null) setFocusedPath(row.parent);
        break;
      case 'Enter':
      case ' ':
        activate(row);
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  if (rootListing?.isPending) {
    return <p className="px-4 py-3 text-xs text-text-muted">Reading folder…</p>;
  }
  if (rootListing?.isError) {
    return <p role="alert" className="px-4 py-3 text-xs text-danger-fg">{rootListing.error.message}</p>;
  }
  if (rows.length === 0) {
    return <p className="px-4 py-3 text-xs text-text-muted">This folder is empty.</p>;
  }

  const activeId = focusedIndex >= 0 ? `explorer-row-${focusedIndex}` : undefined;

  return (
    <div
      ref={listRef}
      role="tree"
      aria-label="Folder"
      tabIndex={0}
      aria-activedescendant={activeId}
      onKeyDown={onKeyDown}
      onFocus={() => setHasFocus(true)}
      onBlur={() => setHasFocus(false)}
      className="h-full overflow-y-auto py-1 outline-hidden"
    >
      {rows.map((row, index) => {
        const { entry } = row;
        const isDir = entry.kind === 'directory';
        const isOpen = isDir && expanded.has(entry.path);
        const listing = isOpen ? listingByPath.get(entry.path) : undefined;
        return (
          <div
            key={entry.path}
            id={`explorer-row-${index}`}
            role="treeitem"
            aria-level={row.depth + 1}
            aria-expanded={isDir ? isOpen : undefined}
            aria-selected={entry.path === openPath}
            data-path={entry.path}
            onClick={() => activate(row)}
            title={entry.ignored ? `${entry.path} (ignored)` : entry.path}
            className={cn(
              'row-hover flex cursor-default items-center gap-1.5 pr-2 text-[13px] text-text-primary',
              entry.ignored && 'opacity-50',
              entry.path === openPath && 'bg-[hsl(var(--text-primary)/0.07)]',
              hasFocus && index === focusedIndex && 'shadow-[inset_0_0_0_1px_hsl(var(--accent)/0.5)]',
            )}
            style={{ height: ROW_HEIGHT, paddingLeft: 8 + row.depth * INDENT_PX }}
          >
            {isDir ? (
              <>
                {isOpen
                  ? <ChevronDown className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
                  : <ChevronRight className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.75} aria-hidden="true" />}
                {isOpen
                  ? <FolderOpen className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.6} aria-hidden="true" />
                  : <Folder className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.6} aria-hidden="true" />}
              </>
            ) : (
              <>
                <span className="w-3.5 shrink-0" aria-hidden="true" />
                <FileGlyph name={entry.name} />
              </>
            )}
            <span className="min-w-0 flex-1 truncate">{entry.name}</span>
            {listing?.isPending && <span className="shrink-0 text-[11px] text-text-muted">…</span>}
            {listing?.isError && <span className="shrink-0 text-[11px] text-danger-fg" title={listing.error.message}>unreadable</span>}
          </div>
        );
      })}
    </div>
  );
}
