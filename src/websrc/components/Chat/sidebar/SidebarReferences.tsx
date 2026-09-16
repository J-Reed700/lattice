import { useEffect, useMemo, useState } from 'react';

import {
  ArrowUpRight,
  Loader2
} from 'lucide-react';
import { useNavigate } from 'react-router';

import { IconButton } from '@/components/ui/IconButton';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { ConversationMessageBookmarkDto } from '@/types';
import {
  chatReferenceKey,
  type CapturedChatReference
} from '@/utils/chatReferenceIndex';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { formatRoleLabel, formatShortRelativeTime, isReferenceInboxEnabled, scrollToMessage } from './sidebarUtils';
import { useCapturedReferencesQuery, useSidebarBookmarksQuery } from './workspaceQueries';

export function SidebarReferences({ query, active }: { query: string; active: boolean }) {
  const navigate = useNavigate();
  const { selectedSpaceId, selectConversation } = useConversationsStore();
  const [referenceInboxEnabled] = useState(isReferenceInboxEnabled);
  const bookmarks = useSidebarBookmarksQuery(query, selectedSpaceId, active);
  const captures = useCapturedReferencesQuery(active && referenceInboxEnabled);
  const snippetResults = useMemo(() => bookmarks.data ?? [], [bookmarks.data]);
  const capturedReferenceIndex = useMemo(() => captures.data ?? new Map<string, CapturedChatReference>(), [captures.data]);
  const isLoadingSnippets = bookmarks.isLoading;
  const isLoadingCaptureIndex = captures.isFetching;
  const [selectedSnippetId, setSelectedSnippetId] = useState<string | null>(null);
  const [snippetRoleFilter, setSnippetRoleFilter] = useState<'all' | 'assistant' | 'user' | 'system'>('all');
  const roleFilteredSnippets = useMemo(() => {
    if (snippetRoleFilter === 'all') return snippetResults;
    return snippetResults.filter((snippet) => snippet.messageRole === snippetRoleFilter);
  }, [snippetResults, snippetRoleFilter]);

  const filteredSnippets = roleFilteredSnippets;

  const snippetCaptureStats = useMemo(() => {
    const total = roleFilteredSnippets.length;
    if (total === 0) {
      return { total: 0, captured: 0, pending: 0 };
    }
    const captured = roleFilteredSnippets.reduce((count, snippet) => {
      const key = chatReferenceKey(snippet.conversationId, snippet.messageId);
      return capturedReferenceIndex.has(key) ? count + 1 : count;
    }, 0);
    return {
      total,
      captured,
      pending: Math.max(0, total - captured),
    };
  }, [capturedReferenceIndex, roleFilteredSnippets]);

  useEffect(() => {
    if (filteredSnippets.length === 0) {
      setSelectedSnippetId(null);
      return;
    }

    const resolved = filteredSnippets.find((snippet) => snippet.id === selectedSnippetId) ?? filteredSnippets[0];
    setSelectedSnippetId(resolved.id);
  }, [filteredSnippets, selectedSnippetId]);

  const openSnippet = async (bookmark: ConversationMessageBookmarkDto) => {
    await selectConversation(bookmark.conversationId);
    scrollToMessage(bookmark.messageId);
  };

  const getCapturedSnippetReference = (
    bookmark: Pick<ConversationMessageBookmarkDto, 'conversationId' | 'messageId'>
  ): CapturedChatReference | null => {
    const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
    return capturedReferenceIndex.get(key) ?? null;
  };

  if (!active) return null;

  return (
    <div className="border-b border-border-subtle pb-2">
      <div className="flex items-center gap-3 px-4 pb-2 pt-3 text-xs">
        {([
          ['all', 'All'],
          ['assistant', 'Assistant'],
          ['user', 'You'],
          ['system', 'System'],
        ] as const).map(([value, label]) => (
          <button
            key={value}
            onClick={() => setSnippetRoleFilter(value)}
            className={`transition-colors duration-fast ${snippetRoleFilter === value
              ? 'text-text-primary'
              : 'text-text-tertiary hover:text-text-primary'
              }`}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="flex items-center justify-between gap-2 px-4 pb-2 text-xs">
        {referenceInboxEnabled ? (
          <span className="flex min-w-0 items-center gap-1 truncate text-text-muted">
            {isLoadingCaptureIndex && <Loader2 className="h-3 w-3 shrink-0 animate-spin" />}
            {snippetCaptureStats.total} {snippetCaptureStats.total === 1 ? 'reference' : 'references'}
            {snippetCaptureStats.captured > 0 ? ` · ${snippetCaptureStats.captured} captured` : ''}
          </span>
        ) : (
          <span />
        )}
        <button
          onClick={() => navigate('/references')}
          className="shrink-0 text-text-secondary transition-colors duration-fast hover:text-text-primary"
        >
          Open References
        </button>
      </div>

      {isLoadingSnippets ? (
        <div className="flex h-12 items-center justify-center text-text-muted">
          <Loader2 className="h-4 w-4 animate-spin" />
        </div>
      ) : filteredSnippets.length === 0 ? (
        <p className="px-4 py-3 text-xs text-text-muted">
          {roleFilteredSnippets.length === 0
            ? 'No references yet.'
            : 'No matches in this filter.'}
        </p>
      ) : (
        <div>
          {filteredSnippets.slice(0, 20).map((bookmark) => {
            const isSelected = bookmark.id === selectedSnippetId;
            const capturedReference = referenceInboxEnabled
              ? getCapturedSnippetReference(bookmark)
              : null;
            return (
              <div
                key={bookmark.id}
                className={`group relative transition-colors duration-fast ${isSelected ? 'bg-surface-raised' : 'hover:bg-surface-raised'
                  }`}
              >
                {isSelected && (
                  <span
                    className="absolute inset-y-0 left-0 w-0.5 bg-accent"
                    aria-hidden="true"
                  />
                )}
                <button
                  onClick={() => setSelectedSnippetId(bookmark.id)}
                  className="w-full px-4 py-2.5 text-left"
                >
                  <p className="truncate text-sm text-text-primary">
                    {bookmark.title || bookmark.conversationTitle}
                  </p>
                  <p className="mt-0.5 truncate text-xs text-text-muted">
                    {formatRoleLabel(bookmark.messageRole)}
                    {capturedReference ? ' · Captured' : ''}
                    {' · '}
                    {formatShortRelativeTime(bookmark.createdAt)}
                  </p>
                  <p className="mt-0.5 truncate text-xs text-text-tertiary">
                    {bookmark.messagePreview}
                  </p>
                </button>
                <div className="pointer-events-none absolute right-2 top-1.5 flex items-center gap-0.5 rounded-sm bg-surface-raised pl-2 opacity-0 transition-opacity duration-fast group-hover:pointer-events-auto group-hover:opacity-100">
                  <IconButton
                    label="Open in chat"
                    onClick={handleAsyncEvent(() => openSnippet(bookmark))}
                  >
                    <ArrowUpRight />
                  </IconButton>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
