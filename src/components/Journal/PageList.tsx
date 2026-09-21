import { useState } from 'react';

import { Pencil, Plus, Trash2 } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

interface PageListProps {
  pages: WorkspaceNote[];
  activePageId: string | null;
  onSelectPage: (noteId: string) => void;
  onNewPage: () => void;
  /** Off when the host already offers a "New page" control of its own. */
  showCreate?: boolean;
  /** How a page is named in the list. Defaults to its own title. */
  displayTitle?: (page: WorkspaceNote) => string;
  onRenamePage?: (noteId: string, title: string) => Promise<unknown> | void;
  onDeletePage?: (page: WorkspaceNote) => Promise<unknown> | void;
}

/** "just now", "12m", "3h", "2d", then a date. Sidebar-width short. */
export function formatPageTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  const minutes = Math.floor((Date.now() - date.getTime()) / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/** A page with no title of its own still needs a name in the list. */
export function pageTitle(page: WorkspaceNote): string {
  return page.title.trim() || 'Untitled page';
}

/**
 * The pages of the workspace, most recently updated first. Every page a
 * synthesis or a capture writes is one click away here — without this list a
 * page like "Week of Sep 1" is written and then unreachable.
 *
 * A row can be renamed in place and deleted; both are offered only when the
 * host passes the handlers.
 */
export function PageList({
  pages,
  activePageId,
  onSelectPage,
  onNewPage,
  showCreate = true,
  displayTitle = pageTitle,
  onRenamePage,
  onDeletePage,
}: PageListProps) {
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  // A journal always keeps one page to write on.
  const canDelete = Boolean(onDeletePage) && pages.length > 1;

  const commitRename = (page: WorkspaceNote) => {
    const next = draft.trim();
    setRenamingId(null);
    if (next && next !== page.title) void onRenamePage?.(page.id, next);
  };

  return (
    <div className="shrink-0">
      <div className="flex h-8 items-center justify-between pl-4 pr-2">
        <h3 className="text-xs font-medium text-text-muted">
          Pages{pages.length > 0 ? <span className="ml-1.5 tabular-nums text-text-disabled">{pages.length}</span> : null}
        </h3>
        {showCreate ? (
          <IconButton label="New page" onClick={onNewPage}>
            <Plus />
          </IconButton>
        ) : null}
      </div>

      {pages.length === 0 ? (
        <p className="px-4 pb-2 text-xs text-text-muted">No pages yet.</p>
      ) : (
        <ul className="px-2 pb-2">
          {pages.map((page) => {
            const isActive = page.id === activePageId;
            const isRenaming = renamingId === page.id;
            const shown = displayTitle(page);
            return (
              <li key={page.id} className="group relative">
                {isRenaming ? (
                  <input
                    autoFocus
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    onFocus={(event) => event.currentTarget.select()}
                    onBlur={() => commitRename(page)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault();
                        commitRename(page);
                      } else if (event.key === 'Escape') {
                        event.preventDefault();
                        setRenamingId(null);
                      }
                    }}
                    maxLength={160}
                    aria-label={`Rename ${shown}`}
                    className="h-8 w-full rounded-md border border-accent/60 bg-surface px-2.5 text-ui text-text-primary outline-none shadow-[0_0_0_3px_hsl(var(--accent)/0.14)]"
                  />
                ) : (
                  <button
                    type="button"
                    onClick={() => onSelectPage(page.id)}
                    onDoubleClick={() => {
                      if (!onRenamePage) return;
                      setDraft(page.title.trim());
                      setRenamingId(page.id);
                    }}
                    aria-current={isActive ? 'page' : undefined}
                    className={`relative flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-left ${
                      isActive ? 'bg-accent-muted' : 'row-hover'
                    }`}
                  >
                    {isActive && (
                      <span
                        className="absolute inset-y-2 left-0 w-[2.5px] rounded-full bg-accent"
                        aria-hidden="true"
                      />
                    )}
                    <span
                      className={`min-w-0 flex-1 truncate text-ui ${
                        isActive ? 'font-medium text-text-primary' : 'text-text-secondary'
                      }`}
                      title={shown}
                    >
                      {shown}
                    </span>
                    <span className="shrink-0 text-[11px] tabular-nums text-text-muted group-focus-within:opacity-0 group-hover:opacity-0">
                      {formatPageTime(page.updatedAt)}
                    </span>
                  </button>
                )}

                {/* Row actions — overlay, reserves no width. */}
                {!isRenaming && (onRenamePage || onDeletePage) ? (
                  <div className="pointer-events-none absolute right-1 top-1/2 flex -translate-y-1/2 items-center gap-0.5 rounded-md bg-surface-overlay pl-1 opacity-0 shadow-control transition-opacity duration-fast group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100">
                    {onRenamePage ? (
                      <IconButton
                        label="Rename page"
                        onClick={() => {
                          setDraft(page.title.trim());
                          setRenamingId(page.id);
                        }}
                      >
                        <Pencil />
                      </IconButton>
                    ) : null}
                    {onDeletePage ? (
                      <IconButton
                        label={canDelete ? 'Delete page' : 'A journal keeps at least one page'}
                        disabled={!canDelete}
                        className="hover:text-[hsl(var(--danger-fg))]"
                        onClick={() => void onDeletePage(page)}
                      >
                        <Trash2 />
                      </IconButton>
                    ) : null}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
