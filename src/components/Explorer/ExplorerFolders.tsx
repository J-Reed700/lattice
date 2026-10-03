import { useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';

import { Database, Folder, FolderX, MoreHorizontal, Pencil, Pin, Search, SlidersHorizontal, Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import { formatFileSize } from '@/utils/files';

import { FolderSettingsDialog } from './FolderSettingsDialog';
import { count, formatEta, indexPercent, tildePath } from './indexProgress';

import type { ExplorerFolder, ExplorerFolders } from './useExplorerFolders';

/** Past this many folders the list gets a filter. */
const FILTER_FROM = 7;

const MENU_ITEM_CLASS =
  'flex w-full items-start gap-2.5 rounded-sm px-2.5 py-2 text-left text-sm text-text-primary transition-colors duration-fast hover:bg-surface focus-visible:bg-surface focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50';

type ChipTone = 'done' | 'paused' | 'live' | 'quiet' | 'alarm';

/** A row's status chip: its words and tone. Exported for tests. */
export function describeFolderChip(folder: ExplorerFolder): { text: string; tone: ChipTone; percent: number | null } {
  const { index } = folder;
  const percent = indexPercent(index.passagesEmbedded, index.passagesTotal);
  if (!folder.exists) return { text: 'Folder missing', tone: 'alarm', percent: null };
  switch (index.state) {
    case 'indexed':
      return { text: `Indexed · ${count(index.filesTotal)} ${index.filesTotal === 1 ? 'file' : 'files'}`, tone: 'done', percent: null };
    case 'partial':
      return { text: `Paused at ${percent}%`, tone: 'paused', percent };
    case 'indexing':
      return {
        text: index.etaSeconds === null ? `Indexing ${percent}%` : `Indexing ${percent}% · ${formatEta(index.etaSeconds)}`,
        tone: 'live',
        percent,
      };
    case 'tooLarge':
      return { text: 'Too large', tone: 'quiet', percent: null };
    case 'error':
      return { text: 'Index error', tone: 'alarm', percent: null };
    case 'notIndexed':
    case 'refused':
      return { text: 'Not indexed', tone: 'quiet', percent: null };
  }
}

/** "just now", "5 min ago", "2 h ago", "3 days ago", then the date. */
export function openedAgo(iso: string, now = Date.now()): string {
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return '';
  const minutes = Math.max(0, Math.round((now - at) / 60_000));
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days} days ago`;
  return new Date(at).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

const plural = (value: number, one: string, many: string) => `${count(value)} ${value === 1 ? one : many}`;

function FolderStatusChip({ folder }: { folder: ExplorerFolder }) {
  const chip = describeFolderChip(folder);
  return (
    <span
      title={folder.index.message ?? undefined}
      data-tone={chip.tone}
      className="relative inline-flex h-[19px] shrink-0 items-center gap-1.5 overflow-hidden rounded-full bg-[hsl(var(--text-primary)/0.055)] px-2 text-[11px] font-medium tabular-nums leading-none text-text-secondary"
    >
      <span
        aria-hidden="true"
        className={cn(
          'h-1.5 w-1.5 shrink-0 rounded-full',
          chip.tone === 'done' && 'bg-[hsl(var(--success))]',
          chip.tone === 'paused' && 'bg-[hsl(var(--warning))]',
          chip.tone === 'live' && 'animate-pulse bg-accent',
          chip.tone === 'quiet' && 'ring-1 ring-inset ring-[hsl(var(--text-muted))]',
          chip.tone === 'alarm' && 'bg-[hsl(var(--danger))]'
        )}
      />
      {chip.text}
      {chip.percent !== null && (
        <span aria-hidden="true" className="absolute inset-x-0 bottom-0 h-[2px] bg-[hsl(var(--text-primary)/0.06)]">
          <span
            className={cn('block h-full transition-[width] duration-300', chip.tone === 'live' ? 'bg-accent' : 'bg-[hsl(var(--warning)/0.7)]')}
            style={{ width: `${chip.percent}%` }}
          />
        </span>
      )}
    </span>
  );
}

interface FolderRowProps {
  folder: ExplorerFolder;
  home: string | null;
  busy: boolean;
  opening: boolean;
  renaming: boolean;
  onOpen: () => void;
  onStartRename: () => void;
  onRename: (_name: string | null) => void;
  onTogglePin: () => void;
  onDeleteIndex: () => void;
  onRemove: () => void;
  onSettings: () => void;
}

function FolderRow({ folder, home, busy, opening, renaming, onOpen, onStartRename, onRename, onTogglePin, onDeleteIndex, onRemove, onSettings }: FolderRowProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const { index } = folder;
  const path = tildePath(folder.root, home);
  const reusedFrom = index.indexRoot === null ? null : tildePath(index.indexRoot, home);
  const hasOwnIndex = index.bytes > 0;
  const missing = !folder.exists;
  const meta = [
    folder.threadCount > 0 ? plural(folder.threadCount, 'thread', 'threads') : null,
    reusedFrom ? `in ${reusedFrom}’s index` : hasOwnIndex ? formatFileSize(index.bytes) : null,
    `opened ${openedAgo(folder.lastOpenedAt)}`,
  ].filter((item): item is string => Boolean(item));

  const body = (
    <>
      <span
        aria-hidden="true"
        className={cn(
          'flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-sunken ring-1 ring-inset ring-border-subtle',
          folder.pinned ? 'text-accent' : 'text-text-muted'
        )}
      >
        {missing ? <FolderX className="h-[18px] w-[18px]" strokeWidth={1.5} /> : <Folder className="h-[18px] w-[18px]" strokeWidth={1.5} />}
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="flex min-w-0 items-center gap-2">
          {renaming ? (
            <RenameField name={folder.name} onDone={onRename} />
          ) : (
            <span className={cn('truncate text-ui font-medium', missing ? 'text-text-tertiary' : 'text-text-primary')}>{folder.name}</span>
          )}
          <FolderStatusChip folder={folder} />
        </span>
        <span className="flex min-w-0 items-center gap-1.5 text-xs text-text-muted">
          <span className="min-w-0 truncate" title={folder.root}>
            {path}
          </span>
          {meta.map((item, position) => (
            <span key={item} className={cn('shrink-0 tabular-nums', position < meta.length - 1 && 'max-sm:hidden')}>
              <span aria-hidden="true" className="mr-1.5">
                ·
              </span>
              {item}
            </span>
          ))}
        </span>
      </span>
    </>
  );

  return (
    <li className={cn('row-hover group flex items-center gap-0.5 pr-2', missing && 'bg-[hsl(var(--text-primary)/0.015)]')}>
      {renaming ? (
        <div className="flex min-w-0 flex-1 items-center gap-3 py-3 pl-4 pr-2">{body}</div>
      ) : (
        <button
          type="button"
          data-folder-open
          disabled={missing || busy}
          aria-busy={opening || undefined}
          onClick={onOpen}
          title={missing ? `${folder.root} is no longer on disk` : `Open ${folder.root}`}
          className="flex min-w-0 flex-1 items-center gap-3 rounded-lg py-3 pl-4 pr-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[hsl(var(--ring)/0.55)] disabled:cursor-default"
        >
          {body}
        </button>
      )}
      <button
        type="button"
        aria-pressed={folder.pinned}
        aria-label={folder.pinned ? `Unpin ${folder.name}` : `Pin ${folder.name}`}
        title={folder.pinned ? 'Unpin' : 'Pin to the top'}
        disabled={busy}
        onClick={onTogglePin}
        className={cn(
          'flex h-7 w-7 shrink-0 items-center justify-center rounded-md transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] disabled:opacity-40',
          folder.pinned ? 'text-accent' : 'text-text-disabled hover:text-text-secondary group-hover:text-text-muted'
        )}
      >
        <Pin className={cn('h-3.5 w-3.5', folder.pinned && 'fill-current')} strokeWidth={1.75} />
      </button>
      <Popover open={menuOpen} onOpenChange={setMenuOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label={`Actions for ${folder.name}`}
            disabled={busy}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] disabled:opacity-40"
          >
            <MoreHorizontal className="h-4 w-4" />
          </button>
        </PopoverTrigger>
        <PopoverContent aria-label={`Actions for ${folder.name}`} align="end" className="w-64 p-1.5">
          <button
            type="button"
            className={MENU_ITEM_CLASS}
            onClick={() => {
              setMenuOpen(false);
              onStartRename();
            }}
          >
            <Pencil className="mt-0.5 h-4 w-4 shrink-0 text-text-muted" strokeWidth={1.75} />
            Rename
          </button>
          <button
            type="button"
            className={MENU_ITEM_CLASS}
            onClick={() => {
              setMenuOpen(false);
              onSettings();
            }}
          >
            <SlidersHorizontal className="mt-0.5 h-4 w-4 shrink-0 text-text-muted" strokeWidth={1.75} />
            <span className="flex min-w-0 flex-col">
              Settings…
              <span className="text-xs text-text-muted">Space and system prompt</span>
            </span>
          </button>
          <button
            type="button"
            className={MENU_ITEM_CLASS}
            disabled={!hasOwnIndex}
            onClick={() => {
              setMenuOpen(false);
              onDeleteIndex();
            }}
          >
            <Database className="mt-0.5 h-4 w-4 shrink-0 text-text-muted" strokeWidth={1.75} />
            <span className="flex min-w-0 flex-col">
              Delete index
              <span className="text-xs text-text-muted">
                {reusedFrom
                  ? `Uses ${reusedFrom}’s index`
                  : hasOwnIndex
                    ? `${formatFileSize(index.bytes)} · built again when it opens`
                    : 'No index to delete'}
              </span>
            </span>
          </button>
          <div className="my-1 h-px bg-border-subtle" />
          <button
            type="button"
            className={cn(MENU_ITEM_CLASS, 'text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))] focus-visible:bg-[hsl(var(--danger-muted))]')}
            onClick={() => {
              setMenuOpen(false);
              onRemove();
            }}
          >
            <Trash2 className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={1.75} />
            Remove…
          </button>
        </PopoverContent>
      </Popover>
    </li>
  );
}

/** The name, edited in place: Enter or leaving keeps it, Escape drops it. */
function RenameField({ name, onDone }: { name: string; onDone: (_name: string | null) => void }) {
  const [value, setValue] = useState(name);
  const done = useRef(false);
  const finish = (next: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(next === null || next.trim() === name ? null : next);
  };
  return (
    <input
      aria-label="Folder name"
      autoFocus
      value={value}
      maxLength={120}
      onChange={(event) => setValue(event.target.value)}
      onFocus={(event) => event.currentTarget.select()}
      onKeyDown={(event) => {
        if (event.key === 'Enter') finish(value);
        else if (event.key === 'Escape') finish(null);
      }}
      onBlur={() => finish(value)}
      className="h-6 min-w-0 flex-1 rounded-md border border-border-default bg-surface px-1.5 text-ui font-medium text-text-primary focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring)/0.5)]"
    />
  );
}

interface RemoveFolderDialogProps {
  folder: ExplorerFolder;
  home: string | null;
  onCancel: () => void;
  onRemove: (_deleteThreads: boolean) => Promise<void>;
}

/** Says what goes and what stays, with the threads kept unless ticked. */
function RemoveFolderDialog({ folder, home, onCancel, onRemove }: RemoveFolderDialogProps) {
  const [deleteThreads, setDeleteThreads] = useState(false);
  const [removing, setRemoving] = useState(false);
  const checkboxId = useId();
  const { index } = folder;
  const indexLine =
    index.indexRoot !== null
      ? `It uses the index of ${tildePath(index.indexRoot, home)}, which stays.`
      : index.bytes > 0
        ? `Its index (${formatFileSize(index.bytes)}) is deleted from Lattice’s data.`
        : 'It has no index to delete.';

  return (
    <Dialog open onOpenChange={(open) => !open && !removing && onCancel()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Remove {folder.name} from Lattice?</DialogTitle>
          <DialogDescription>
            {indexLine} The folder on disk isn&apos;t touched.
          </DialogDescription>
        </DialogHeader>
        {folder.threadCount > 0 && (
          <label htmlFor={checkboxId} className="flex cursor-pointer items-start gap-3 rounded-lg bg-surface-sunken px-3 py-2.5 ring-1 ring-inset ring-border-subtle">
            <input
              id={checkboxId}
              type="checkbox"
              checked={deleteThreads}
              disabled={removing}
              onChange={(event) => setDeleteThreads(event.target.checked)}
              className="mt-0.5 h-4 w-4 shrink-0 accent-[hsl(var(--danger))]"
            />
            <span className="flex flex-col text-sm text-text-primary">
              Also delete its {plural(folder.threadCount, 'chat thread', 'chat threads')}
              <span className="text-xs text-text-muted">
                {deleteThreads ? 'Their messages go too. This can’t be undone.' : 'Kept threads come back if you add the folder again.'}
              </span>
            </span>
          </label>
        )}
        <DialogFooter>
          <Button type="button" variant="outline" disabled={removing} onClick={onCancel}>
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={removing}
            onClick={async () => {
              setRemoving(true);
              try {
                await onRemove(deleteThreads);
              } finally {
                setRemoving(false);
              }
            }}
          >
            {removing ? 'Removing…' : 'Remove'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export interface YourFoldersProps {
  list: ExplorerFolders;
  /** The root being opened, if any: the page is busy until it resolves. */
  opening: string | null;
  onOpen: (_folder: ExplorerFolder) => void;
}

/**
 * "Your folders": every folder picked in the Explorer, pinned first, then
 * the most recently opened. Each row says where its index stands and opens
 * the folder on click; the pin and the menu (Rename, Settings…, Delete index, Remove…)
 * sit at its end. Arrow keys move between rows.
 */
export function YourFolders({ list, opening, onOpen }: YourFoldersProps) {
  const [query, setQuery] = useState('');
  const [renaming, setRenaming] = useState<string | null>(null);
  const [removing, setRemoving] = useState<ExplorerFolder | null>(null);
  const [editing, setEditing] = useState<ExplorerFolder | null>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const folders = useMemo(() => list.folders ?? [], [list.folders]);

  const totalBytes = folders.reduce((sum, folder) => sum + folder.index.bytes, 0);
  const totals = [plural(folders.length, 'folder', 'folders'), totalBytes > 0 ? `${formatFileSize(totalBytes)} of indexes` : null]
    .filter(Boolean)
    .join(' · ');

  const needle = query.trim().toLowerCase();
  const shown = needle
    ? folders.filter((folder) =>
        [folder.name, folder.root, tildePath(folder.root, list.home)].some((text) => text.toLowerCase().includes(needle)))
    : folders;

  const moveFocus = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const rows = Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>('[data-folder-open]') ?? []);
    const at = rows.indexOf(document.activeElement as HTMLButtonElement);
    if (at < 0) return;
    event.preventDefault();
    rows[Math.min(rows.length - 1, Math.max(0, at + (event.key === 'ArrowDown' ? 1 : -1)))]?.focus();
  };

  if (folders.length === 0) return null;

  return (
    <section aria-labelledby="explorer-your-folders" className="mt-10">
      <div className="flex items-baseline justify-between gap-3 px-1">
        <h2 id="explorer-your-folders" className="text-[11px] font-medium uppercase tracking-[.08em] text-text-muted">
          Your folders
        </h2>
        <p className="text-[11.5px] tabular-nums text-text-muted">{totals}</p>
      </div>
      {folders.length >= FILTER_FROM && (
        <div className="relative mt-2.5">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
          <input
            type="search"
            aria-label="Filter folders"
            placeholder="Filter folders"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="h-8 w-full rounded-lg border border-border-subtle bg-surface pl-8 pr-3 text-ui text-text-primary placeholder:text-text-muted focus:border-border-default focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring)/0.35)]"
          />
        </div>
      )}
      {shown.length > 0 ? (
        <ul
          ref={listRef}
          onKeyDown={moveFocus}
          className="mt-2.5 divide-y divide-border-subtle overflow-hidden rounded-xl bg-surface shadow-sheet"
        >
          {shown.map((folder) => (
            <FolderRow
              key={folder.root}
              folder={folder}
              home={list.home}
              busy={list.busy.has(folder.root) || opening !== null}
              opening={opening === folder.root}
              renaming={renaming === folder.root}
              onOpen={() => onOpen(folder)}
              onStartRename={() => setRenaming(folder.root)}
              onRename={(name) => {
                setRenaming(null);
                if (name !== null) void list.rename(folder.root, name);
              }}
              onTogglePin={() => void list.setPinned(folder.root, !folder.pinned)}
              onDeleteIndex={() => void list.deleteIndex(folder.root)}
              onRemove={() => setRemoving(folder)}
              onSettings={() => setEditing(folder)}
            />
          ))}
        </ul>
      ) : (
        <p className="mt-2.5 rounded-xl px-4 py-6 text-center text-sm text-text-muted ring-1 ring-inset ring-border-subtle">
          No folders match “{query.trim()}”.
        </p>
      )}
      {editing && (
        <FolderSettingsDialog
          folder={editing}
          onCancel={() => setEditing(null)}
          onSave={(instructions, spaceId) => list.saveSettings(editing.root, instructions, spaceId)}
        />
      )}
      {removing && (
        <RemoveFolderDialog
          folder={removing}
          home={list.home}
          onCancel={() => setRemoving(null)}
          onRemove={async (deleteThreads) => {
            if (await list.remove(removing.root, deleteThreads)) setRemoving(null);
          }}
        />
      )}
    </section>
  );
}
