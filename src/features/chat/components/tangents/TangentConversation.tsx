import { useEffect, useMemo, useRef, type ReactNode } from 'react';

import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { fetchBookmarks, fetchConversationDetail, fetchMessages } from '@/features/chat/api/conversationQueryData';
import { deriveMessageMetadata } from '@/features/chat/controller/messageMetadata';
import { reconcilePersistedFailedMessages } from '@/features/chat/controller/optimisticMessages';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { useStopSpeechOnChange } from '@/features/speech/hooks/useSpeech';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { ConversationSnapshotProvider, useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { ConversationsState, RegenerateOptions } from '@/shared/conversations/conversationsStore.types';
import { conversationDraftKey, setDraft } from '@/shared/drafts/drafts';
import type { ConversationMessage } from '@/types/conversation';

const EMPTY_MESSAGES: ConversationMessage[] = [];

/**
 * A tangent's conversation, for the shared chat inside it. Only the view of
 * the controller changes: the chat reads this conversation without selecting
 * it, and every action and all streaming state still belong to the one
 * ConversationsProvider above both panels.
 */
export function TangentConversation({ conversationId: id, inheritedCount, onTurnEnd, children }: {
  conversationId: string;
  /** How many leading messages the tangent inherited; its own thread starts after them. */
  inheritedCount: number;
  /** Called each time a turn in this tangent has ended. */
  onTurnEnd?: () => void;
  children: ReactNode;
}) {
  const api = useApi();
  const base = useConversationsStore();
  const messagesQuery = useQuery({ queryKey: conversationKeys.messages(id), queryFn: () => fetchMessages(id, api) });
  const detailQuery = useQuery({ queryKey: conversationKeys.detail(id), queryFn: () => fetchConversationDetail(id, api) });
  const bookmarksQuery = useQuery({ queryKey: conversationKeys.bookmarks(id), queryFn: () => fetchBookmarks(id, undefined, api) });
  const persisted = messagesQuery.data ?? EMPTY_MESSAGES;
  const metadata = useMemo(() => deriveMessageMetadata(persisted), [persisted]);
  const sending = base.inFlightGenerations.has(id);
  const wasSending = useRef(false);
  useStopSpeechOnChange(id);

  useEffect(() => {
    if (!messagesQuery.data) return;
    conversationUiStore.setState((state) => {
      const optimisticMessages = reconcilePersistedFailedMessages(state.optimisticMessages, id, messagesQuery.data!);
      return optimisticMessages === state.optimisticMessages ? state : { optimisticMessages };
    });
  }, [id, messagesQuery.data]);

  useEffect(() => {
    if (wasSending.current && !sending) onTurnEnd?.();
    wasSending.current = sending;
  }, [onTurnEnd, sending]);

  const scope = useMemo<ConversationsState>(() => {
    const draftKey = conversationDraftKey(id);
    return {
      ...base,
      activeConversationId: id,
      conversations: detailQuery.data ? [{ ...detailQuery.data, messages: persisted.slice(inheritedCount) }] : [],
      lastMessageSources: metadata.sources,
      messageVerification: metadata.verification,
      messageRetrieval: metadata.retrieval,
      messageTurn: metadata.turn,
      messageBookmarks: bookmarksQuery.data ?? [],
      messageBookmarkMap: new Map((bookmarksQuery.data ?? []).map((bookmark) => [bookmark.messageId, bookmark])),
      composerDraft: null,
      setComposerDraft: (value: string | null) => setDraft(draftKey, value ?? ''),
      regenerateResponse: async (conversationId: string, options?: RegenerateOptions) => {
        const outcome = await base.regenerateResponse(conversationId, options);
        if (outcome === 'failed') setDraft(draftKey, [...persisted].reverse().find((message) => message.role === 'user')?.content ?? '');
        return outcome;
      },
    };
  }, [base, bookmarksQuery.data, detailQuery.data, id, inheritedCount, metadata, persisted]);

  if (messagesQuery.data && detailQuery.data) return <ConversationSnapshotProvider value={scope}>{children}</ConversationSnapshotProvider>;
  if (messagesQuery.isError || detailQuery.isError || (detailQuery.isSuccess && !detailQuery.data)) return (
    <div role="alert" className="p-4 text-sm">
      <p>Couldn’t open this tangent.</p>
      <button type="button" className="mt-2 text-accent underline" onClick={() => { void messagesQuery.refetch(); void detailQuery.refetch(); }}>Try again</button>
    </div>
  );
  return <p role="status" className="p-4 text-sm text-text-muted">Opening tangent…</p>;
}
