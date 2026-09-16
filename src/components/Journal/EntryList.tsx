import { useMemo, useState } from 'react';

import {
  differenceInCalendarDays,
  format,
  isSameMonth,
  isToday,
  isYesterday,
  startOfMonth,
} from 'date-fns';
import { PanelLeft, Plus } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { SidebarHeader, SidebarSearch, SidebarTabs } from '@/components/ui/SidebarHeader';
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
  onNewEntry: () => void;
  onRenameEntry: (entryId: string, title: string) => Promise<void> | void;
  onDeleteEntry: (entryId: string) => Promise<void> | void;
  onToggleCollapse: () => void;
  /** Workspace pages, most recently updated first. */
  pages: WorkspaceNote[];
  activePageId: string | null;
  onSelectPage: (noteId: string) => void;
  onNewPage: () => void;
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

  return (
    <aside className="journal-index flex h-full w-[256px] 2xl:w-[280px] shrink-0 flex-col border-r border-border-subtle bg-surface">
      <SidebarHeader
        title="Journal"
        actions={
          <>
            <IconButton label="New entry" shortcut="⌘N" onClick={onNewEntry}>
              <Plus />
            </IconButton>
            <JournalCalendarPopover
              entries={entries}
              onJumpToEntry={(id) => setSelectedId(id)}
            />
            <IconButton label="Hide sidebar" shortcut="⌘\" onClick={onToggleCollapse}>
              <PanelLeft />
            </IconButton>
          </>
        }
      />

      <div className="notebook-cover mx-4 mb-4 mt-3 shrink-0 overflow-hidden rounded-xl p-5">
        <div className="relative z-10 flex items-center justify-between text-[9px] font-medium uppercase tracking-[0.2em]">
          <span>Personal notebook</span><span aria-hidden="true">✳</span>
        </div>
        <h2 className="relative z-10 mt-7 max-w-[180px] break-words font-serif text-[29px] leading-[1.12] tracking-tight">{currentJournal?.name || 'Your journal'}</h2>
        <div className="relative z-10 mt-6 flex items-center justify-between border-t border-current/20 pt-3 text-[10px]">
          <span>{pages.length} {pages.length === 1 ? 'page' : 'pages'} · {entries.length} {entries.length === 1 ? 'entry' : 'entries'}</span>
          <span aria-hidden="true">✳</span>
        </div>
      </div>
      {/* Journal switcher */}
      <div className="shrink-0 border-b border-border-subtle px-4 py-2">
        <JournalPickerMenu
          journals={journals}
          currentJournal={currentJournal}
          onSwitch={onSwitchJournal}
          onCreate={onCreateJournal}
          onRenameCurrent={onRenameJournal}
          onDeleteCurrent={onDeleteJournal}
        />
      </div>

      <button type="button" onClick={onNewPage} className="journal-new-page mx-4 my-4 flex shrink-0 items-center justify-between rounded-lg px-4 py-3 text-xs font-medium transition-colors">
        Write a new page <Plus className="h-4 w-4" />
      </button>
      <PageList
        pages={pages}
        activePageId={activePageId}
        onSelectPage={onSelectPage}
        onNewPage={onNewPage}
      />

      <div className="shrink-0 space-y-2.5 border-b border-border-subtle px-4 pb-2.5 pt-2">
        {/* Named so the two lists in this sidebar are told apart at a glance. */}
        <h3 className="text-[10px] font-medium uppercase tracking-[0.12em] text-text-tertiary">Entries</h3>
        <SidebarSearch value={search} onChange={setSearch} placeholder="Search this journal" />
        <SidebarTabs value={filter} onChange={setFilter} options={FILTER_OPTIONS} />
      </div>

      {/* Entry list */}
      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <p className="px-4 py-6 text-sm text-text-muted">Loading…</p>
        ) : loadError ? (
          <p className="px-4 py-6 text-sm text-[hsl(var(--danger-fg))]">{loadError}</p>
        ) : entries.length === 0 ? (
          <div className="px-4 py-8">
            <p className="text-sm font-medium text-text-secondary">{search.trim() || filter !== 'all' ? 'No matching entries' : 'Room for a new thought'}</p>
            <p className="mt-2 text-xs leading-relaxed text-text-tertiary">{search.trim() || filter !== 'all' ? 'Try another search or view all entries.' : 'Start a conversation here, then bring what matters into your pages.'}</p>
            <button type="button" onClick={search.trim() || filter !== 'all' ? () => { setSearch(''); setFilter('all'); } : onNewEntry} className="mt-4 rounded-md bg-accent-muted px-3 py-2 text-xs font-medium text-accent hover:bg-accent hover:text-accent-fg">
              {search.trim() || filter !== 'all' ? 'Clear filters' : 'Create an entry'}
            </button>
          </div>
        ) : (
          <div className="pb-2">
            {groups.map((group) => (
              <div key={group.key}>
                <h3 className="px-4 pb-1 pt-4 text-[10px] font-medium uppercase tracking-[0.12em] text-text-tertiary">
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

      {/* Footer */}
      <div className="flex h-8 shrink-0 items-center justify-between border-t border-border-subtle px-4">
        <button
          type="button"
          onClick={() => {
            const today = entries.find((e) => {
              const d = new Date(e.updatedAt);
              return !Number.isNaN(d.getTime()) && isToday(d);
            });
            if (today) setSelectedId(today.id);
          }}
          className="text-xs text-text-muted transition-colors duration-fast hover:text-text-primary"
          title="Jump to today"
        >
          Today
        </button>
        <span className="text-xs text-text-muted">
          {entries.length} entr{entries.length === 1 ? 'y' : 'ies'}
        </span>
      </div>
    </aside>
  );
}
