import { useMemo, useState } from 'react';

import { Check, ChevronRight } from 'lucide-react';

import { SidebarSearch } from '@/components/ui/SidebarHeader';
import { ConversationActionsMenu, ConversationRenameField, type ConversationMenuServices } from '@/features/chat/components/sidebar/ConversationActionsMenu';
import { formatShortRelativeTime, getRecencyGroup } from '@/features/chat/components/sidebar/sidebarUtils';
import type { ConversationRename } from '@/features/chat/components/sidebar/useConversationRename';
import { cn } from '@/lib/utils';
import type { Conversation } from '@/types/conversation';

export interface ThreadGroup {
  key: string;
  label: string;
  items: Conversation[];
}

/** Title or last message contains the query, ignoring case. */
export function matchesThread(thread: Conversation, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return thread.title.toLowerCase().includes(needle)
    || (thread.lastMessagePreview ?? '').toLowerCase().includes(needle);
}

/** Pinned first, then the Chat sidebar's date buckets, then by month; newest first in each. */
export function groupThreads(threads: readonly Conversation[], now = new Date()): ThreadGroup[] {
  const newestFirst = [...threads].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  const pinned = newestFirst.filter((thread) => thread.isPinned);
  const groups: ThreadGroup[] = pinned.length > 0 ? [{ key: 'pinned', label: 'Pinned', items: pinned }] : [];
  const byKey = new Map<string, ThreadGroup>();
  for (const thread of newestFirst) {
    if (thread.isPinned) continue;
    const updatedAt = new Date(thread.updatedAt);
    const { key, label } = getRecencyGroup(Number.isNaN(updatedAt.getTime()) ? now : updatedAt, now);
    let group = byKey.get(key);
    if (!group) {
      group = { key, label, items: [] };
      byKey.set(key, group);
      groups.push(group);
    }
    group.items.push(thread);
  }
  return groups;
}

interface ExplorerThreadListProps {
  threads: Conversation[];
  archived: Conversation[];
  activeId: string | null;
  services: ConversationMenuServices;
  rename: ConversationRename;
  deletingId: string | null;
  onSelect: (id: string) => void;
  onDelete: (thread: Conversation) => void;
}

/** A folder's threads: searchable, pinned first, grouped by date, archived ones behind a toggle. */
export function ExplorerThreadList({ threads, archived, activeId, services, rename, deletingId, onSelect, onDelete }: ExplorerThreadListProps) {
  const [query, setQuery] = useState('');
  const [showArchived, setShowArchived] = useState(() => archived.some((thread) => thread.id === activeId));
  const [menuId, setMenuId] = useState<string | null>(null);
  const groups = useMemo(() => groupThreads(threads.filter((thread) => matchesThread(thread, query))), [query, threads]);
  const shownArchived = useMemo(() => archived.filter((thread) => matchesThread(thread, query)), [archived, query]);

  const row = (thread: Conversation) => {
    if (rename.renamingId === thread.id) {
      return (
        <div key={thread.id} className="px-1.5 py-1">
          <ConversationRenameField conversation={thread} rename={rename} />
        </div>
      );
    }
    const active = thread.id === activeId;
    const menuOpen = menuId === thread.id;
    return (
      <div key={thread.id} className="group flex items-center rounded-sm hover:bg-[hsl(var(--text-primary)/0.06)] focus-within:bg-[hsl(var(--text-primary)/0.06)]">
        <button
          type="button"
          onClick={() => onSelect(thread.id)}
          aria-current={active ? 'true' : undefined}
          className={cn(
            'flex min-w-0 flex-1 items-center gap-2 py-1.5 pl-2.5 pr-1 text-left text-[13px] focus-visible:outline-hidden',
            active ? 'text-text-primary' : 'text-text-secondary',
          )}
        >
          <Check className={cn('h-3.5 w-3.5 shrink-0 text-accent', !active && 'invisible')} strokeWidth={2} aria-hidden="true" />
          <span className="min-w-0 flex-1 truncate" title={thread.title}>{thread.title}</span>
          <span className="shrink-0 text-[11px] tabular-nums text-text-muted">{formatShortRelativeTime(thread.updatedAt)}</span>
        </button>
        <div className={cn('shrink-0 pr-0.5 transition-opacity duration-fast', menuOpen ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100')}>
          <ConversationActionsMenu
            conversation={thread}
            services={services}
            open={menuOpen}
            onOpenChange={(open) => setMenuId(open ? thread.id : null)}
            onRename={() => rename.begin(thread.id, thread.title)}
            onDelete={() => onDelete(thread)}
            deleting={deletingId === thread.id}
            deleteLabel="Delete conversation…"
          />
        </div>
      </div>
    );
  };

  return (
    <div className="flex max-h-[min(32rem,70vh)] flex-col">
      <div className="shrink-0 space-y-1.5 border-b border-border-subtle p-2">
        <div className="flex items-baseline justify-between px-0.5">
          <p className="text-[11px] font-medium uppercase tracking-[.08em] text-text-muted">Threads in this folder</p>
          <span className="text-[11px] tabular-nums text-text-muted">{threads.length}</span>
        </div>
        <SidebarSearch value={query} onChange={setQuery} placeholder="Search threads" />
      </div>
      <nav aria-label="Threads" className="min-h-0 flex-1 overflow-y-auto p-1">
        {groups.map((group) => (
          <section key={group.key} aria-label={group.label}>
            <p className="px-2.5 pb-1 pt-2 text-xs font-medium text-text-muted">{group.label}</p>
            {group.items.map(row)}
          </section>
        ))}
        {groups.length === 0 && (
          <p className="px-2.5 py-3 text-[13px] text-text-muted">{query.trim() ? 'No threads match.' : 'No open threads.'}</p>
        )}
        {archived.length > 0 && (
          <section className="mt-1 border-t border-border-subtle pt-1">
            <button
              type="button"
              aria-expanded={showArchived}
              onClick={() => setShowArchived((shown) => !shown)}
              className="flex w-full items-center gap-1.5 rounded-sm px-2.5 py-1.5 text-left text-xs font-medium text-text-muted hover:bg-[hsl(var(--text-primary)/0.06)] focus-visible:bg-[hsl(var(--text-primary)/0.06)] focus-visible:outline-hidden"
            >
              <ChevronRight className={cn('h-3.5 w-3.5 shrink-0 transition-transform duration-fast', showArchived && 'rotate-90')} strokeWidth={1.75} aria-hidden="true" />
              Archived ({shownArchived.length})
            </button>
            {showArchived && shownArchived.map(row)}
          </section>
        )}
      </nav>
    </div>
  );
}
