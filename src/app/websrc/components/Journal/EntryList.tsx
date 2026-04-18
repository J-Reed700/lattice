import { useMemo, useState } from 'react';

import {
  differenceInCalendarDays,
  format,
  isSameMonth,
  isToday,
  isYesterday,
  startOfMonth,
} from 'date-fns';
import { NotebookPen, PanelLeft, Plus, Search } from 'lucide-react';

import type { ConversationJournalDto } from '@/types/api/conversation';

import { EntryListItem } from './EntryListItem';
import { JournalCalendarPopover } from './JournalCalendarPopover';
import { JournalPickerMenu } from './JournalPickerMenu';

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
 * picker, new-entry button, search, filter chips, grouped entry list, footer.
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
    <aside className="flex h-full w-[280px] shrink-0 flex-col border-r border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))]">
      {/* Top rail */}
      <div className="flex h-12 items-center justify-between border-b border-[hsl(var(--border-subtle))] px-4">
        <h2 className="font-serif text-sm font-semibold text-[hsl(var(--text-primary))]">
          Journal
        </h2>
        <div className="flex items-center gap-0.5">
          <JournalCalendarPopover
            entries={entries}
            onJumpToEntry={(id) => setSelectedId(id)}
          />
          <button
            type="button"
            onClick={onToggleCollapse}
            className="rounded-sm p-1.5 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            aria-label="Collapse sidebar"
            title="Collapse sidebar"
          >
            <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
          </button>
        </div>
      </div>

      {/* Journal picker */}
      <div className="border-b border-[hsl(var(--border-subtle))] px-4 py-3">
        <JournalPickerMenu
          journals={journals}
          currentJournal={currentJournal}
          onSwitch={onSwitchJournal}
          onCreate={onCreateJournal}
          onRenameCurrent={onRenameJournal}
          onDeleteCurrent={onDeleteJournal}
        />
      </div>

      {/* New Entry + search + filters */}
      <div className="space-y-2 border-b border-[hsl(var(--border-subtle))] px-4 py-3">
        <button
          type="button"
          onClick={onNewEntry}
          className="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md bg-[hsl(var(--accent))] text-sm font-medium text-[hsl(var(--accent-fg))] hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
        >
          <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
          New Entry
        </button>
        <div className="relative">
          <Search
            className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[hsl(var(--text-tertiary))]"
            strokeWidth={1.75}
          />
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search this journal..."
            className="h-8 w-full rounded-sm border border-[hsl(var(--border-default))] bg-[hsl(var(--bg))] pl-8 pr-2 text-sm text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] outline-none focus:border-[hsl(var(--accent))] transition-colors duration-fast"
          />
        </div>
        <div className="flex items-center gap-3 text-xs">
          {FILTER_OPTIONS.map((option) => {
            const isActive = filter === option.id;
            return (
              <button
                key={option.id}
                type="button"
                onClick={() => setFilter(option.id)}
                className={`pb-1 border-b-2 transition-colors duration-fast ${
                  isActive
                    ? 'border-[hsl(var(--accent))] text-[hsl(var(--text-primary))]'
                    : 'border-transparent text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))]'
                }`}
              >
                {option.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* Entry list */}
      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <p className="px-4 py-6 text-center text-xs text-[hsl(var(--text-muted))]">
            Loading entries…
          </p>
        ) : loadError ? (
          <p className="px-4 py-6 text-center text-xs text-[hsl(var(--danger-fg))]">
            {loadError}
          </p>
        ) : entries.length === 0 ? (
          <div className="flex flex-col items-center gap-4 px-6 py-10 text-center">
            <NotebookPen
              className="h-8 w-8 text-[hsl(var(--text-muted))]"
              strokeWidth={1.5}
            />
            <p className="text-sm text-[hsl(var(--text-tertiary))]">
              {search.trim() || filter !== 'all' ? 'No entries match.' : 'No entries yet.'}
            </p>
            {!search.trim() && filter === 'all' && (
              <p className="max-w-[240px] text-xs text-[hsl(var(--text-muted))]">
                Start a conversation in this journal — it'll show up here as a dated entry.
                Or write directly in the editor.
              </p>
            )}
          </div>
        ) : (
          <div className="pb-2">
            {groups.map((group, groupIndex) => (
              <div key={group.key}>
                <h3
                  className={`px-4 py-2 font-serif italic text-xs text-[hsl(var(--text-tertiary))] ${
                    groupIndex > 0 ? 'mt-4' : ''
                  }`}
                >
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
      <div className="flex h-8 items-center justify-between border-t border-[hsl(var(--border-subtle))] px-4">
        <button
          type="button"
          onClick={() => {
            const today = entries.find((e) => {
              const d = new Date(e.updatedAt);
              return !Number.isNaN(d.getTime()) && isToday(d);
            });
            if (today) setSelectedId(today.id);
          }}
          className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
          title="Jump to today"
        >
          Today
        </button>
        <span className="text-xxs text-[hsl(var(--text-muted))]">
          {entries.length} entr{entries.length === 1 ? 'y' : 'ies'}
        </span>
      </div>
    </aside>
  );
}
