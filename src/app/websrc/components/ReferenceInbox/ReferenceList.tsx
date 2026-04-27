import { useMemo } from 'react';

import {
  differenceInCalendarDays,
  format,
  isSameMonth,
  isToday,
  isYesterday,
  startOfMonth,
} from 'date-fns';
import { Bookmark, PanelLeft, Search } from 'lucide-react';

import type { ConversationMessageBookmarkDto } from '@/types';
import { chatReferenceKey } from '@/utils/chatReferenceIndex';

import { ReferenceListItem } from './ReferenceListItem';
import { ReferenceOriginPicker } from './ReferenceOriginPicker';

import type {
  OriginFilter,
  StatusChip,
  UseReferenceInboxResult,
} from './useReferenceInbox';

interface ReferenceListProps {
  state: UseReferenceInboxResult;
  onToggleCollapse: () => void;
  onOpenInOrigin: (bookmark: ConversationMessageBookmarkDto) => void;
  onCopy: (bookmark: ConversationMessageBookmarkDto) => void;
  onDelete: (bookmark: ConversationMessageBookmarkDto) => void;
}

type GroupKey = 'today' | 'yesterday' | 'thisWeek' | 'thisMonth' | string;

interface Group {
  key: GroupKey;
  label: string;
  items: ConversationMessageBookmarkDto[];
}

function groupByDate(bookmarks: ConversationMessageBookmarkDto[]): Group[] {
  const now = new Date();
  const groups = new Map<GroupKey, Group>();

  const push = (key: GroupKey, label: string, bookmark: ConversationMessageBookmarkDto) => {
    let group = groups.get(key);
    if (!group) {
      group = { key, label, items: [] };
      groups.set(key, group);
    }
    group.items.push(bookmark);
  };

  for (const bookmark of bookmarks) {
    const date = new Date(bookmark.createdAt);
    if (Number.isNaN(date.getTime())) {
      push('unknown', 'Earlier', bookmark);
      continue;
    }
    if (isToday(date)) push('today', 'Today', bookmark);
    else if (isYesterday(date)) push('yesterday', 'Yesterday', bookmark);
    else if (differenceInCalendarDays(now, date) < 7) push('thisWeek', 'This week', bookmark);
    else if (isSameMonth(date, now)) push('thisMonth', 'This month', bookmark);
    else {
      const monthStart = startOfMonth(date);
      const key = format(monthStart, 'yyyy-MM');
      const label = format(monthStart, 'MMM yyyy');
      push(key, label, bookmark);
    }
  }

  const order: GroupKey[] = ['today', 'yesterday', 'thisWeek', 'thisMonth'];
  const fixed: Group[] = [];
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

const FILTER_OPTIONS: Array<{ id: StatusChip; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'recent', label: 'Recent' },
  { id: 'captured', label: 'Captured' },
];

/**
 * Full left-sidebar for the reference inbox: wordmark top rail, origin picker,
 * search, filter chips, grouped reference list, footer.
 * Spec §4.
 */
export function ReferenceList({
  state,
  onToggleCollapse,
  onOpenInOrigin,
  onCopy,
  onDelete,
}: ReferenceListProps) {
  const {
    filteredBookmarks,
    journalsById,
    capturedIndex,
    isLoading,
    query,
    setQuery,
    originFilter,
    setOriginFilter,
    statusChip,
    setStatusChip,
    selectedId,
    setSelectedId,
  } = state;

  const groups = useMemo(() => groupByDate(filteredBookmarks), [filteredBookmarks]);

  const isEmpty = !isLoading && filteredBookmarks.length === 0;
  const isFilteredEmpty =
    isEmpty && (query.trim() !== '' || originFilter !== 'all' || statusChip !== 'all');

  const handleOriginChange = (value: OriginFilter) => setOriginFilter(value);

  return (
    <aside className="flex h-full w-[280px] shrink-0 flex-col border-r border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))]">
      {/* Top rail */}
      <div className="flex h-12 items-center justify-between border-b border-[hsl(var(--border-subtle))] px-4">
        <h2 className="font-serif text-sm font-semibold text-[hsl(var(--text-primary))]">
          References
        </h2>
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

      {/* Origin picker */}
      <div className="border-b border-[hsl(var(--border-subtle))] px-4 py-3">
        <ReferenceOriginPicker value={originFilter} onChange={handleOriginChange} />
      </div>

      {/* Search + chips */}
      <div className="space-y-2 border-b border-[hsl(var(--border-subtle))] px-4 py-3">
        <div className="relative">
          <Search
            className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[hsl(var(--text-tertiary))]"
            strokeWidth={1.75}
          />
          <input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search references..."
            className="h-8 w-full rounded-sm border border-[hsl(var(--border-default))] bg-[hsl(var(--bg))] pl-8 pr-2 text-sm text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] outline-none focus:border-[hsl(var(--accent))] transition-colors duration-fast"
          />
        </div>
        <div className="flex items-center gap-3 text-xs">
          {FILTER_OPTIONS.map((option) => {
            const isActive = statusChip === option.id;
            return (
              <button
                key={option.id}
                type="button"
                onClick={() => setStatusChip(option.id)}
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

      {/* List body */}
      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <p className="px-4 py-6 text-center text-xs text-[hsl(var(--text-muted))]">
            Loading references…
          </p>
        ) : isEmpty ? (
          <div className="flex flex-col items-center gap-4 px-6 py-10 text-center">
            <Bookmark
              className="h-8 w-8 text-[hsl(var(--text-muted))]"
              strokeWidth={1.5}
            />
            {isFilteredEmpty ? (
              <p className="text-sm text-[hsl(var(--text-tertiary))]">
                No references match.
              </p>
            ) : (
              <>
                <p className="text-sm text-[hsl(var(--text-tertiary))]">
                  Nothing saved yet.
                </p>
                <p className="max-w-[240px] text-xs text-[hsl(var(--text-muted))]">
                  Bookmark any message in Chat using the bookmark icon — it'll collect
                  here so you can come back to it.
                </p>
              </>
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
                {group.items.map((bookmark) => {
                  const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
                  const isCaptured = capturedIndex.has(key);
                  const isJournalOrigin = journalsById.has(bookmark.spaceId);
                  return (
                    <ReferenceListItem
                      key={bookmark.id}
                      bookmark={bookmark}
                      isActive={selectedId === bookmark.id}
                      isCaptured={isCaptured}
                      isJournalOrigin={isJournalOrigin}
                      onSelect={() => setSelectedId(bookmark.id)}
                      onOpenInOrigin={() => onOpenInOrigin(bookmark)}
                      onCopy={() => onCopy(bookmark)}
                      onDelete={() => onDelete(bookmark)}
                    />
                  );
                })}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="flex h-8 items-center justify-end border-t border-[hsl(var(--border-subtle))] px-4">
        <span className="text-xxs text-[hsl(var(--text-muted))]">
          {filteredBookmarks.length} reference{filteredBookmarks.length === 1 ? '' : 's'}
        </span>
      </div>
    </aside>
  );
}
