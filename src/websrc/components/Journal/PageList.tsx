import { Plus } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

interface PageListProps {
  pages: WorkspaceNote[];
  activePageId: string | null;
  onSelectPage: (noteId: string) => void;
  onNewPage: () => void;
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
 */
export function PageList({ pages, activePageId, onSelectPage, onNewPage }: PageListProps) {
  return (
    <div className="shrink-0 border-b border-border-subtle">
      <div className="flex h-9 items-center justify-between pl-4 pr-2">
        <h3 className="text-[10px] font-medium uppercase tracking-[0.12em] text-text-tertiary">Pages</h3>
        <IconButton label="New page" onClick={onNewPage}>
          <Plus />
        </IconButton>
      </div>

      {pages.length === 0 ? (
        <p className="px-4 pb-2 text-xs text-text-muted">No pages yet.</p>
      ) : (
        <ul className="max-h-[220px] overflow-y-auto px-2 pb-3">
          {pages.map((page) => {
            const isActive = page.id === activePageId;
            return (
              <li key={page.id}>
                <button
                  type="button"
                  onClick={() => onSelectPage(page.id)}
                  aria-current={isActive ? 'page' : undefined}
                  className={`relative flex w-full items-baseline gap-2 rounded-md px-3 py-2.5 text-left transition-colors duration-fast ${
                    isActive ? 'bg-accent-muted' : 'hover:bg-surface-raised'
                  }`}
                >
                  {isActive && (
                    <span
                      className="absolute inset-y-0 left-0 w-0.5 bg-accent"
                      aria-hidden="true"
                    />
                  )}
                  <span
                    className={`min-w-0 flex-1 truncate text-sm text-text-primary${
                      isActive ? ' font-medium' : ''
                    }`}
                    title={pageTitle(page)}
                  >
                    {pageTitle(page)}
                  </span>
                  <span className="shrink-0 text-xs tabular-nums text-text-muted">
                    {formatPageTime(page.updatedAt)}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
