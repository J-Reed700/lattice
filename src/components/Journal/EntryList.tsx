import { cloneElement, type ReactElement, useMemo, useState } from 'react';

import {
  differenceInCalendarDays,
  format,
  isSameMonth,
  isToday,
  isYesterday,
  startOfMonth,
} from 'date-fns';
import { MessageCircle, PanelLeft, PenLine, Plus } from 'lucide-react';

import { SpacePickerPopover, useOpenSpaces } from '@/components/Chat/SpacePickerPopover';
import { IconButton } from '@/components/ui/IconButton';
import { SidebarSearch, SidebarTabs } from '@/components/ui/SidebarHeader';
import type { ConversationJournalDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { EntryListItem } from './EntryListItem';
import { JournalCalendarPopover } from './JournalCalendarPopover';
import { JournalPickerMenu } from './JournalPickerMenu';
import { PageList } from './PageList';

import type { EntryFilter, JournalEntrySummary, UseJournalEntriesResult } from './useJournalEntries';

interface EntryListProps {
  entriesState: UseJournalEntriesResult;
  journals: ConversationJournalDto[];
  currentJournal: ConversationJournalDto | null;
  onSwitchJournal: (journalId: string) => void;
  onCreateJournal: () => void;
  onRenameJournal: (nextName: string) => Promise<void> | void;
  onDeleteJournal: () => Promise<void> | void;
  /** Start a chat filed under this journal, searching the given space. */
  onNewEntry: (_spaceId?: string) => void;
  onRenameEntry: (entryId: string, title: string) => Promise<void> | void;
  onDeleteEntry: (entryId: string) => Promise<void> | void;
  onToggleCollapse: () => void;
  /** Workspace pages, most recently updated first. */
  pages: WorkspaceNote[];
  activePageId: string | null;
  onSelectPage: (noteId: string) => void;
  onNewPage: () => void;
  onRenamePage: (noteId: string, title: string) => Promise<unknown> | void;
  onDeletePage: (page: WorkspaceNote) => Promise<unknown> | void;
  /** How a page is named in the list (the journal's first page has a legacy title). */
  displayPageTitle: (page: WorkspaceNote) => string;
}

type GroupKey = 'pinned' | 'today' | 'yesterday' | 'thisWeek' | 'thisMonth' | string;

interface EntryGroup {
  key: GroupKey;
  label: string;
  entries: JournalEntrySummary[];
}

function groupEntries(entries: JournalEntrySummary[], pinnedIds: Set<string>): EntryGroup[] {
  const now = new Date();
  const groups = new Map<GroupKey, EntryGroup>();

  const pushGroup = (key: GroupKey, label: string, entry: JournalEntrySummary) => {
    let group = groups.get(key);
    if (!group) {
      group = { key, label, entries: [] };
      groups.set(key, group);
    }
    group.entries.push(entry);
  };

  for (const entry of entries) {
    if (pinnedIds.has(entry.id)) {
      pushGroup('pinned', 'Pinned', entry);
      continue;
    }
    const date = new Date(entry.updatedAt);
    if (Number.isNaN(date.getTime())) {
      pushGroup('unknown', 'Earlier', entry);
      continue;
    }
    if (isToday(date)) {
      pushGroup('today', 'Today', entry);
    } else if (isYesterday(date)) {
      pushGroup('yesterday', 'Yesterday', entry);
    } else if (differenceInCalendarDays(now, date) < 7) {
      pushGroup('thisWeek', 'This week', entry);
    } else if (isSameMonth(date, now)) {
      pushGroup('thisMonth', 'This month', entry);
    } else {
      const monthStart = startOfMonth(date);
      const key = format(monthStart, 'yyyy-MM');
      const label = format(monthStart, 'MMM yyyy');
      pushGroup(key, label, entry);
    }
  }

  const order: GroupKey[] = ['pinned', 'today', 'yesterday', 'thisWeek', 'thisMonth'];
  const fixed: EntryGroup[] = [];
  for (const k of order) {
    const group = groups.get(k);
    if (group) fixed.push(group);
  }
  const monthGroups = [...groups.values()]
    .filter((g) => !order.includes(g.key) && g.key !== 'unknown')
    .sort((a, b) => (a.key > b.key ? -1 : 1));
  const unknownGroup = groups.get('unknown');
  return [
    ...fixed,
    ...monthGroups,
    ...(unknownGroup ? [unknownGroup] : []),
  ];
}

const FILTER_OPTIONS: Array<{ id: EntryFilter; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'pinned', label: 'Pinned' },
  { id: 'today', label: 'Today' },
];

/**
 * Full left-sidebar for the journal workspace: wordmark top rail, journal
 * picker, page list, search, filter chips, grouped entry list, footer.
 * Spec §4.
 */
/**
 * The way into a new chat from the journal. With more than one space it asks
 * which documents the chat should search, because a journal is not a space and
 * nothing on this screen implies one. These chats used to be filed into
 * whatever space the Chat sidebar last had selected, so the same button
 * searched a different library depending on a choice made on another screen.
 */
function NewConversationButton({
  onNewEntry,
  children,
}: {
  onNewEntry: (_spaceId?: string) => void;
  children: ReactElement<{ onClick?: () => void }>;
}) {
  const spaces = useOpenSpaces();
  // One space is no choice, and a menu with one row is a second click for nothing.
  if (spaces.length <= 1) {
    return cloneElement(children, { onClick: () => onNewEntry() });
  }
  return (
    <SpacePickerPopover
      heading="Which documents should it search?"
      side="bottom"
      align="end"
      onSelect={(spaceId) => onNewEntry(spaceId)}
    >
      {children}
    </SpacePickerPopover>
  );
}

export function EntryList({
  entriesState,
  journals,
  currentJournal,
  onSwitchJournal,
  onCreateJournal,
  onRenameJournal,
  onDeleteJournal,
  onNewEntry,
  onRenameEntry,
  onDeleteEntry,
  onToggleCollapse,
  pages,
  activePageId,
  onSelectPage,
  onNewPage,
  onRenamePage,
  onDeletePage,
  displayPageTitle,
}: EntryListProps) {
  const {
    entries,
    isLoading,
    loadError,
    search,
    setSearch,
    filter,
    setFilter,
    pinnedIds,
    togglePinned,
    selectedId,
    setSelectedId,
  } = entriesState;

  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');

  const groups = useMemo(() => groupEntries(entries, pinnedIds), [entries, pinnedIds]);

  const commitRename = async (entryId: string, originalTitle: string) => {
    const next = renameDraft.trim();
    if (next && next !== originalTitle) {
      await onRenameEntry(entryId, next);
    }
    setRenamingId(null);
    setRenameDraft('');
  };

  const counts = `${pages.length} ${pages.length === 1 ? 'page' : 'pages'}`;

  return (
    <aside className="journal-index flex h-full w-[264px] shrink-0 flex-col border-r border-border-subtle 2xl:w-[288px]">
      {/* The notebook: its cover, its name, and the way to another one. */}
      <div className="shrink-0 px-2.5 pb-2 pt-2.5">
        <JournalPickerMenu
          journals={journals}
          currentJournal={currentJournal}
          onSwitch={onSwitchJournal}
          onCreate={onCreateJournal}
          onRenameCurrent={onRenameJournal}
          onDeleteCurrent={onDeleteJournal}
          meta={counts}
        />
        <div className="mt-2 flex items-center gap-1">
          <button
            type="button"
            onClick={onNewPage}
            className="journal-new-page pressable flex h-8 min-w-0 flex-1 items-center justify-center gap-1.5 rounded-md px-3 text-ui font-medium shadow-action transition-[background-color,scale] duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-bg"
          >
            <PenLine className="h-3.5 w-3.5" strokeWidth={1.75} />
            New page
          </button>
          <JournalCalendarPopover
            entries={entries}
            onJumpToEntry={(id) => setSelectedId(id)}
          />
          <IconButton label="Hide sidebar" shortcut="⌘\" onClick={onToggleCollapse}>
            <PanelLeft />
          </IconButton>
        </div>
      </div>

      {/* One scroll for both lists, so neither is trapped in a little window. */}
      <div className="min-h-0 flex-1 overflow-y-auto pb-3">
        <PageList
          pages={pages}
          activePageId={activePageId}
          onSelectPage={onSelectPage}
          onNewPage={onNewPage}
          showCreate={false}
          displayTitle={displayPageTitle}
          onRenamePage={onRenamePage}
          onDeletePage={onDeletePage}
        />

        <div className="mt-3 border-t border-border-subtle pt-2">
          {/* Called what they are. "Entries" read as things you write; these are
              AI conversations filed under the journal, and you write in Chat. */}
          <div className="flex h-8 items-center justify-between pl-4 pr-2">
            <h3 className="flex items-center gap-1.5 text-xs font-medium text-text-muted">
              <MessageCircle className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
              Conversations{entries.length > 0 ? <span className="tabular-nums text-text-disabled">{entries.length}</span> : null}
            </h3>
            <NewConversationButton onNewEntry={onNewEntry}>
              <IconButton label="Ask something in Chat, filed under this journal">
                <Plus />
              </IconButton>
            </NewConversationButton>
          </div>
          <p className="px-4 pb-2 text-[11px] leading-snug text-text-muted">
            Chats with your library, filed here. Pick one to read it beside your page.
          </p>
          {/* Finding tools only once there is something to find. */}
          {entries.length > 0 || search.trim() || filter !== 'all' ? (
            <div className="space-y-2 px-4 pb-1">
              <SidebarSearch value={search} onChange={setSearch} placeholder="Search conversations" />
              <SidebarTabs value={filter} onChange={setFilter} options={FILTER_OPTIONS} />
            </div>
          ) : null}
        </div>

      {/* Entry list */}
      <div>
        {isLoading ? (
          <p className="px-4 py-6 text-sm text-text-muted">Loading…</p>
        ) : loadError ? (
          <p className="px-4 py-6 text-sm text-[hsl(var(--danger-fg))]">{loadError}</p>
        ) : entries.length === 0 ? (
          <div className="px-4 py-8">
            <p className="text-sm font-medium text-text-secondary">{search.trim() || filter !== 'all' ? 'No matching conversations' : 'No conversations yet'}</p>
            <p className="mt-2 text-xs leading-relaxed text-text-tertiary">{search.trim() || filter !== 'all' ? 'Try another search or view them all.' : 'Ask your library a question in Chat and it is filed here, ready to read beside a page.'}</p>
            {search.trim() || filter !== 'all' ? (
              <button type="button" onClick={() => { setSearch(''); setFilter('all'); }} className="mt-4 rounded-md bg-accent-muted px-3 py-2 text-xs font-medium text-accent hover:bg-accent hover:text-accent-fg">
                Clear filters
              </button>
            ) : (
              <NewConversationButton onNewEntry={onNewEntry}>
                <button type="button" className="mt-4 rounded-md bg-accent-muted px-3 py-2 text-xs font-medium text-accent hover:bg-accent hover:text-accent-fg">
                  Ask in Chat
                </button>
              </NewConversationButton>
            )}
          </div>
        ) : (
          <div className="pb-2">
            {groups.map((group) => (
              <div key={group.key}>
                <h3 className="px-4 pb-1 pt-3 text-[11px] font-medium text-text-muted">
                  {group.label}
                </h3>
                {group.entries.map((entry) => (
                  <EntryListItem
                    key={entry.id}
                    entry={entry}
                    isActive={selectedId === entry.id}
                    isPinned={pinnedIds.has(entry.id)}
                    isRenaming={renamingId === entry.id}
                    renameDraft={renameDraft}
                    onSelect={() => setSelectedId(entry.id)}
                    onTogglePinned={() => togglePinned(entry.id)}
                    onStartRename={() => {
                      setRenamingId(entry.id);
                      setRenameDraft(entry.title);
                    }}
                    onCommitRename={() => void commitRename(entry.id, entry.title)}
                    onCancelRename={() => {
                      setRenamingId(null);
                      setRenameDraft('');
                    }}
                    onRenameDraftChange={setRenameDraft}
                    onDelete={() => void onDeleteEntry(entry.id)}
                  />
                ))}
              </div>
            ))}
          </div>
        )}
      </div>

      </div>
    </aside>
  );
}
