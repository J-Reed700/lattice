import { useMemo } from 'react';

import {
  differenceInCalendarDays,
  format,
  isSameMonth,
  isToday,
  isYesterday,
  startOfMonth,
} from 'date-fns';
import { PanelLeft } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { SidebarHeader, SidebarSearch, SidebarTabs } from '@/components/ui/SidebarHeader';
import { usePassageReferencesQuery } from '@/hooks/queries/usePassageReferencesQuery';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { PassageReferenceDto } from '@/types/api/references';
import { chatReferenceKey } from '@/utils/chatReferenceIndex';

import { PassageListItem } from './PassageListItem';
import { ReferenceListItem } from './ReferenceListItem';
import { ReferenceOriginPicker } from './ReferenceOriginPicker';

import type { InboxItem } from './inboxItems';
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
  onOpenPassageSource: (passage: PassageReferenceDto) => void;
  onCopyPassage: (passage: PassageReferenceDto) => void;
  onDeletePassage: (passage: PassageReferenceDto) => void;
}

type GroupKey = 'today' | 'yesterday' | 'thisWeek' | 'thisMonth' | string;

interface Group {
  key: GroupKey;
  label: string;
  items: InboxItem[];
}

function groupByDate(items: InboxItem[]): Group[] {
  const now = new Date();
  const groups = new Map<GroupKey, Group>();

  const push = (key: GroupKey, label: string, item: InboxItem) => {
    let group = groups.get(key);
    if (!group) {
      group = { key, label, items: [] };
      groups.set(key, group);
    }
    group.items.push(item);
  };

  for (const item of items) {
    const date = new Date(item.createdAt);
    if (Number.isNaN(date.getTime())) {
      push('unknown', 'Earlier', item);
      continue;
    }
    if (isToday(date)) push('today', 'Today', item);
    else if (isYesterday(date)) push('yesterday', 'Yesterday', item);
    else if (differenceInCalendarDays(now, date) < 7) push('thisWeek', 'This week', item);
    else if (isSameMonth(date, now)) push('thisMonth', 'This month', item);
    else {
      const monthStart = startOfMonth(date);
      const key = format(monthStart, 'yyyy-MM');
      const label = format(monthStart, 'MMM yyyy');
      push(key, label, item);
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
  onOpenPassageSource,
  onCopyPassage,
  onDeletePassage,
}: ReferenceListProps) {
  const {
    filteredItems,
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

  // The passage list is React Query's; when it fails, an empty shelf would
  // read as "you have saved nothing" instead of "we could not look".
  const passagesQuery = usePassageReferencesQuery();

  const groups = useMemo(() => groupByDate(filteredItems), [filteredItems]);

  const isEmpty = !isLoading && filteredItems.length === 0;
  const hasLoadError = passagesQuery.isError;
  const isFilteredEmpty =
    isEmpty && (query.trim() !== '' || originFilter !== 'all' || statusChip !== 'all');

  const handleOriginChange = (value: OriginFilter) => setOriginFilter(value);

  return (
    <aside className="flex h-full w-[280px] shrink-0 flex-col border-r border-border-subtle bg-bg">
      <SidebarHeader
        title="References"
        actions={
          <IconButton label="Hide sidebar" shortcut="⌘\" onClick={onToggleCollapse}>
            <PanelLeft />
          </IconButton>
        }
      />

      {/* Origin — one line */}
      <div className="shrink-0 border-b border-border-subtle px-4 py-2">
        <ReferenceOriginPicker value={originFilter} onChange={handleOriginChange} />
      </div>

      <div className="shrink-0 space-y-2.5 border-b border-border-subtle px-4 py-2.5">
        <SidebarSearch value={query} onChange={setQuery} placeholder="Search references" />
        <SidebarTabs value={statusChip} onChange={setStatusChip} options={FILTER_OPTIONS} />
      </div>

      {/* List body */}
      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <p className="px-4 py-6 text-sm text-text-muted">Loading…</p>
        ) : isEmpty && hasLoadError ? (
          <p className="px-4 py-6 text-sm text-text-secondary">
            Couldn&apos;t load references.{' '}
            <button
              type="button"
              onClick={() => void passagesQuery.refetch()}
              disabled={passagesQuery.isFetching}
              className="text-accent underline-offset-2 hover:underline disabled:opacity-60"
            >
              Retry
            </button>
          </p>
        ) : isEmpty ? (
          <p className="px-4 py-6 text-sm text-text-secondary">
            {isFilteredEmpty ? 'No references match.' : 'No references yet.'}
          </p>
        ) : (
          <div className="pb-2">
            {groups.map((group, groupIndex) => (
              <div key={group.key}>
                {/* REFERENCE-REDESIGN-SPEC §4.3: serif italic, not an
                    uppercase tracked label. */}
                <h3
                  className={`px-4 py-2 font-serif text-xs italic text-text-tertiary ${
                    groupIndex > 0 ? 'mt-4' : ''
                  }`}
                >
                  {group.label}
                </h3>
                {group.items.map((item) => {
                  if (item.kind === 'passage') {
                    const { passage } = item;
                    return (
                      <PassageListItem
                        key={passage.id}
                        passage={passage}
                        isActive={selectedId === passage.id}
                        onSelect={() => setSelectedId(passage.id)}
                        onOpenSource={() => onOpenPassageSource(passage)}
                        onCopy={() => onCopyPassage(passage)}
                        onDelete={() => onDeletePassage(passage)}
                      />
                    );
                  }
                  const { bookmark } = item;
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
      <div className="flex h-8 shrink-0 items-center justify-end border-t border-border-subtle px-4">
        <span className="text-xs text-text-muted">
          {filteredItems.length} reference{filteredItems.length === 1 ? '' : 's'}
        </span>
      </div>
    </aside>
  );
}
