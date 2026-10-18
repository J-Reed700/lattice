import { useMemo } from 'react';

import { useQuery } from '@tanstack/react-query';
import { ArrowUpRight, Search } from 'lucide-react';

import { fetchMessages } from '@/features/chat/api/conversationQueryData';
import { useTangentActions } from '@/features/chat/hooks/useTangentActions';
import type { ConversationTangentDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ConversationMessage } from '@/types/conversation';

import { groupTangents, recentTangents, type TangentGroup, type TangentListView, type TangentSort } from './tangentGroups';
import { flatNode, TangentRow } from './TangentRow';

const NO_MESSAGES: ConversationMessage[] = [];
const SORTS: { value: TangentSort; label: string }[] = [
  { value: 'conversation', label: 'Conversation order' },
  { value: 'recent', label: 'Recent activity' },
];
const LABEL_CLASS = 'text-[11px] font-medium uppercase tracking-wide text-text-muted';
const LINK_CLASS = 'inline-flex shrink-0 items-center gap-1 rounded px-1 text-xs text-text-secondary hover:text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring';

/** The tangent sidebar's list: filter, order, and one group per source reply. */
export function TangentList({ conversationId, tangents, drafts, view, onViewChange, onOpen, onShowInChat }: {
  conversationId: string;
  tangents: readonly ConversationTangentDto[];
  drafts: Map<string, string>;
  view: TangentListView;
  onViewChange: (view: TangentListView) => void;
  onOpen: (tangentId: string) => void;
  onShowInChat: (messageId: string) => void;
}) {
  const actions = useTangentActions(drafts);
  // The chat's own messages query. The chat keeps it fresh, so reading it here
  // never fetches again; it only loads when nothing has yet.
  const messages = useQuery({
    queryKey: conversationKeys.messages(conversationId),
    queryFn: () => fetchMessages(conversationId),
    staleTime: Infinity,
  }).data ?? NO_MESSAGES;
  const groups = useMemo(() => (view.sort === 'conversation' ? groupTangents(tangents, messages, view) : null), [messages, tangents, view]);
  const recent = useMemo(() => (view.sort === 'recent' ? recentTangents(tangents, view) : null), [tangents, view]);
  const empty = groups ? groups.length === 0 : recent?.length === 0;
  const update = (patch: Partial<TangentListView>) => onViewChange({ ...view, ...patch });

  const header = (group: TangentGroup) => {
    const { source } = group;
    if (source.kind === 'reply') return <>
      <div className="flex items-center justify-between gap-2">
        <p className={LABEL_CLASS}>From the reply</p>
        <button type="button" className={LINK_CLASS} onClick={() => onShowInChat(source.messageId)}>
          Show in chat<ArrowUpRight className="h-3 w-3" aria-hidden="true" />
        </button>
      </div>
      <p className="mt-1 line-clamp-2 border-l-2 border-border-subtle pl-2 font-serif text-xs leading-relaxed text-text-secondary">{source.excerpt}</p>
    </>;
    if (source.kind === 'tangent') return <div className="flex items-center justify-between gap-2">
      <p className={`${LABEL_CLASS} min-w-0 truncate`}>From a reply in “{source.tangent.title}”</p>
      <button type="button" className={LINK_CLASS} onClick={() => onOpen(source.tangent.conversationId)}>Open tangent</button>
    </div>;
    return <p className={LABEL_CLASS}>From replies no longer in this conversation</p>;
  };

  return <div className="flex min-h-0 flex-1 flex-col">
    <div className="shrink-0 space-y-2 border-b border-border-subtle p-3">
      <div className="relative">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted" aria-hidden="true" />
        <input type="search" value={view.query} aria-label="Filter tangents" placeholder="Filter by title or passage"
          className="w-full rounded-lg border border-border-subtle bg-surface py-1.5 pl-8 pr-2 text-sm outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          onChange={event => update({ query: event.target.value })}
          onKeyDown={event => {
            // The first Escape clears the filter; the next one closes the sidebar.
            if (event.key === 'Escape' && view.query) { event.preventDefault(); update({ query: '' }); }
          }} />
      </div>
      <div role="group" aria-label="Order tangents" className="flex rounded-lg border border-border-subtle p-0.5 text-xs">
        {SORTS.map(sort => <button key={sort.value} type="button" aria-pressed={view.sort === sort.value} onClick={() => update({ sort: sort.value })}
          className={`flex-1 rounded-md px-2 py-1 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${view.sort === sort.value ? 'bg-surface font-medium text-text-primary' : 'text-text-muted hover:text-text-secondary'}`}>
          {sort.label}
        </button>)}
      </div>
      {view.focusMessageId && <div className="flex items-center justify-between gap-2 text-xs text-text-muted">
        <span>Showing one reply’s tangents</span>
        <button type="button" className={LINK_CLASS} onClick={() => update({ focusMessageId: null })}>Show all</button>
      </div>}
    </div>
    <div className="min-h-0 flex-1 overflow-y-auto p-3">
      {empty && <div role="status" className="space-y-2 p-1 text-sm text-text-muted">
        <p>{view.query.trim() ? `No tangents match “${view.query.trim()}”.` : 'No tangents from this reply.'}</p>
        <button type="button" className="text-accent" onClick={() => update(view.query.trim() ? { query: '' } : { focusMessageId: null })}>
          {view.query.trim() ? 'Clear filter' : 'Show all tangents'}
        </button>
      </div>}
      {groups && <div className="space-y-5">{groups.map(group => <section key={group.key} aria-label="Tangents from one reply" data-tangent-group={group.key}>
        <div className="mb-2">{header(group)}</div>
        <ul className="space-y-2">
          {group.nodes.map(node => <TangentRow key={node.tangent.conversationId} node={node} actions={actions} onOpen={onOpen} />)}
        </ul>
      </section>)}</div>}
      {recent && <ul className="space-y-2">
        {recent.map(tangent => <TangentRow key={tangent.conversationId} node={flatNode(tangent)} actions={actions} onOpen={onOpen} />)}
      </ul>}
    </div>
  </div>;
}
