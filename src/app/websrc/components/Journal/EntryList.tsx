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
    <aside className="flex h-full w-[280px] shrink-0 flex-col border-r border-border-subtle bg-surface">
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

      {/* Journal picker — one line */}
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

      <PageList
        pages={pages}
        activePageId={activePageId}
        onSelectPage={onSelectPage}
        onNewPage={onNewPage}
      />

      <div className="shrink-0 space-y-2.5 border-b border-border-subtle px-4 pb-2.5 pt-2">
        {/* Named so the two lists in this sidebar are told apart at a glance. */}
        <h3 className="font-serif text-xs italic text-text-tertiary">Entries</h3>
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
          <p className="px-4 py-6 text-sm text-text-secondary">
            {search.trim() || filter !== 'all' ? 'No entries match.' : 'No entries yet.'}
          </p>
        ) : (
          <div className="pb-2">
            {groups.map((group) => (
              <div key={group.key}>
                <h3 className="px-4 pb-1 pt-4 font-serif text-xs italic text-text-tertiary">
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
