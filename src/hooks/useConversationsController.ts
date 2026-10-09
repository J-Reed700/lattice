import { useCallback, useEffect, useMemo } from 'react';

import { useQueries, useQuery, useQueryClient } from '@tanstack/react-query';

import { spacesQueryOptions } from '@/features/spaces/api/queries';
import type {
  ConversationsState,
} from '@/stores/conversationsStore.types';
import { conversationUiStore, useConversationUiStore } from '@/stores/conversationUiStore';
import type { Conversation } from '@/types/conversation';

import { createConversationLifecycleRegistry } from './conversations/lifecycleRegistry';
import { deriveMessageMetadata } from './conversations/messageMetadata';
import { reconcilePersistedFailedMessages } from './conversations/optimisticMessages';
import { MAX_OBSERVED_CONVERSATIONS, MAX_OBSERVED_MEMBERSHIPS, useConversationActions } from './conversations/useConversationActions';
import { useConversationTurnLifecycle } from './conversations/useConversationTurnLifecycle';
import { conversationKeys, type ConversationListParams } from './queries/conversationKeys';
import {
  fetchBookmarks, fetchConversationDetail, fetchConversationList, fetchLinkedDocuments,
  fetchMemberships, fetchMessages, fetchWebSources,
} from './queries/conversationQueryData';

export { conversationKeys } from './queries/conversationKeys';
export { MAX_OBSERVED_CONVERSATIONS, MAX_OBSERVED_MEMBERSHIPS } from './conversations/useConversationActions';

export function useConversationsController(): ConversationsState {
  const queryClient = useQueryClient();
  const lifecycle = useMemo(createConversationLifecycleRegistry, []);
  const addRequestedId = useCallback((
    field:
      | 'requestedLinkedConversationIds'
      | 'requestedWebSourceConversationIds'
      | 'requestedMembershipDocumentIds',
    id: string
  ): void => {
    conversationUiStore.setState(state => {
      const next = new Set(state[field]);
      // Re-inserting moves the id to the newest end, so the set is an LRU.
      next.delete(id);
      next.add(id);
      // Membership panels also outlive the documents that requested them unless
      // evicted. Removing the observer lets React Query reclaim inactive data.
      const capacity = field === 'requestedMembershipDocumentIds'
        ? MAX_OBSERVED_MEMBERSHIPS
        : MAX_OBSERVED_CONVERSATIONS;
      for (const oldest of next) {
        if (next.size <= capacity) break;
        next.delete(oldest);
      }
      return { [field]: next };
    });
  }, []);
  const ui = useConversationUiStore();
  const listParams = useMemo<ConversationListParams>(() => ({
    spaceId: ui.selectedSpaceId,
    filterMode: ui.filterMode,
    searchQuery: ui.searchQuery,
  }), [ui.filterMode, ui.searchQuery, ui.selectedSpaceId]);

  const spacesQuery = useQuery(spacesQueryOptions());
  const conversationsQuery = useQuery({
    queryKey: conversationKeys.list(listParams),
    queryFn: () => fetchConversationList(listParams),
    staleTime: 15_000,
  });
  const activeId = ui.activeConversationId;
  const detailQuery = useQuery({
    queryKey: conversationKeys.detail(activeId ?? ''),
    queryFn: () => fetchConversationDetail(activeId!),
    enabled: Boolean(activeId),
    staleTime: 30_000,
  });
  const messagesQuery = useQuery({
    queryKey: conversationKeys.messages(activeId ?? ''),
    queryFn: () => fetchMessages(activeId!),
    enabled: Boolean(activeId),
    staleTime: 10_000,
  });
  useEffect(() => () => lifecycle.cleanupAll(), [lifecycle]);
  useEffect(() => {
    if (!activeId || !messagesQuery.data) return;
    conversationUiStore.setState(state => {
      const optimisticMessages = reconcilePersistedFailedMessages(state.optimisticMessages, activeId, messagesQuery.data!);
      return optimisticMessages === state.optimisticMessages ? state : { optimisticMessages };
    });
  }, [activeId, messagesQuery.data]);
  const bookmarksQuery = useQuery({
    queryKey: conversationKeys.bookmarks(activeId ?? ''),
    queryFn: () => fetchBookmarks(activeId!),
    enabled: Boolean(activeId),
    staleTime: 10_000,
  });

  const linkedIds = useMemo(
    () => [...ui.requestedLinkedConversationIds].sort(),
    [ui.requestedLinkedConversationIds]
  );
  const webSourceIds = useMemo(
    () => [...ui.requestedWebSourceConversationIds].sort(),
    [ui.requestedWebSourceConversationIds]
  );
  const membershipIds = useMemo(
    () => [...ui.requestedMembershipDocumentIds].sort(),
    [ui.requestedMembershipDocumentIds]
  );
  const linkedQueries = useQueries({
    queries: linkedIds.map(id => ({
      queryKey: conversationKeys.linkedDocuments(id),
      queryFn: () => fetchLinkedDocuments(id),
      staleTime: 10_000,
    })),
  });
  const webSourceQueries = useQueries({
    queries: webSourceIds.map(id => ({
      queryKey: conversationKeys.webSources(id),
      queryFn: () => fetchWebSources(id),
      staleTime: 10_000,
    })),
  });
  const membershipQueries = useQueries({
    queries: membershipIds.map(id => ({
      queryKey: conversationKeys.memberships(id),
      queryFn: () => fetchMemberships(id),
      staleTime: 10_000,
    })),
  });

  const conversations = useMemo<Conversation[]>(() => {
    const list = [...(conversationsQuery.data ?? [])];
    if (activeId && detailQuery.data && !detailQuery.data.tangentParentId && !list.some(item => item.id === activeId)) {
      list.unshift(detailQuery.data);
    }
    return list.map(conversation => conversation.id === activeId
      ? {
          ...conversation,
          messages: messagesQuery.data ?? [],
          // The explorer list query does not carry the compaction record, so the
          // active row takes it from the detail read; without this the divider
          // vanishes on reload even though the summary is still applied.
          compaction: detailQuery.data?.compaction ?? conversation.compaction,
        }
      : conversation);
  }, [activeId, conversationsQuery.data, detailQuery.data, messagesQuery.data]);
  const messageBookmarks = useMemo(
    () => bookmarksQuery.data ?? [],
    [bookmarksQuery.data]
  );
  const messageBookmarkMap = useMemo(
    () => new Map(messageBookmarks.map(bookmark => [bookmark.messageId, bookmark] as const)),
    [messageBookmarks]
  );
  const messageMetadata = useMemo(
    () => deriveMessageMetadata(messagesQuery.data ?? []),
    [messagesQuery.data]
  );
  const linkedDocumentsByConversationId = useMemo(
    () => new Map(linkedIds.map((id, index) => [id, linkedQueries[index]?.data ?? []] as const)),
    [linkedIds, linkedQueries]
  );
  const webSourcesByConversationId = useMemo(
    () => new Map(webSourceIds.map((id, index) => [id, webSourceQueries[index]?.data ?? []] as const)),
    [webSourceIds, webSourceQueries]
  );
  const documentSpaceMembershipsByDocumentId = useMemo(
    () => new Map(membershipIds.map((id, index) => [id, membershipQueries[index]?.data ?? []] as const)),
    [membershipIds, membershipQueries]
  );

  const actions = useConversationActions({ queryClient, addRequestedId, lifecycle });
  const {
    invalidateLists, loadSpaces, setSelectedSpace, setFilterMode, setSearchQuery,
    loadConversations, loadMessageBookmarks, createConversation,
    setConversationSaved, setConversationBookmarked, setConversationPinned,
    setConversationArchived, renameConversation, bookmarkMessage, unbookmarkMessage,
    moveConversationToSpace, loadConversationLinkedDocuments, loadConversationWebSources,
    addConversationWebSource, removeConversationWebSource, removeConversationLinkedDocument,
    loadDocumentSpaceMemberships, setDocumentSpaceMembership, selectConversation,
    truncateAfter, forkConversation, continueInNewConversation, compactConversation,
    deleteMessage, deleteConversation, dismissFailedMessage, setComposerDraft, clearError,
  } = actions;
  const { sendMessage, retryFailedMessage, regenerateResponse, cancelGeneration } = useConversationTurnLifecycle({
    queryClient, conversations, invalidateLists, addRequestedId, lifecycle,
  });

  const queryError = conversationsQuery.error ?? spacesQuery.error ?? messagesQuery.error ?? bookmarksQuery.error;

  return useMemo<ConversationsState>(() => ({
    spaces: spacesQuery.data ?? [],
    selectedSpaceId: ui.selectedSpaceId,
    filterMode: ui.filterMode,
    searchQuery: ui.searchQuery,
    conversations,
    messageBookmarks,
    messageBookmarkMap,
    activeConversationId: ui.activeConversationId,
    isLoading: conversationsQuery.isLoading || messagesQuery.isLoading,
    isLoadingBookmarks: bookmarksQuery.isLoading,
    inFlightGenerations: ui.inFlightGenerations,
    error: ui.error ?? queryError?.message ?? null,
    optimisticMessages: ui.optimisticMessages,
    lastMessageSources: messageMetadata.sources,
    messageVerification: messageMetadata.verification,
    messageRetrieval: messageMetadata.retrieval,
    messageTurn: messageMetadata.turn,
    liveRetrieval: ui.liveRetrieval,
    liveSteps: ui.liveSteps,
    composerDraft: ui.composerDraft,
    linkedDocumentsByConversationId,
    webSourcesByConversationId,
    documentSpaceMembershipsByDocumentId,
    loadSpaces,
    setSelectedSpace,
    setFilterMode,
    setSearchQuery,
    loadConversations,
    loadMessageBookmarks,
    createConversation,
    setConversationSaved,
    setConversationBookmarked,
    setConversationPinned,
    setConversationArchived,
    renameConversation,
    bookmarkMessage,
    unbookmarkMessage,
    moveConversationToSpace,
    loadConversationLinkedDocuments,
    loadConversationWebSources,
    addConversationWebSource,
    removeConversationWebSource,
    removeConversationLinkedDocument,
    loadDocumentSpaceMemberships,
    setDocumentSpaceMembership,
    selectConversation,
    sendMessage,
    retryFailedMessage,
    regenerateResponse,
    truncateAfter,
    forkConversation,
    continueInNewConversation,
    compactConversation,
    setComposerDraft,
    cancelGeneration,
    deleteMessage,
    deleteConversation,
    dismissFailedMessage,
    clearError,
  }), [
    addConversationWebSource, bookmarkMessage, bookmarksQuery.isLoading, cancelGeneration,
    clearError, compactConversation, continueInNewConversation, conversations, conversationsQuery.isLoading,
    createConversation, deleteConversation, dismissFailedMessage,
    deleteMessage, documentSpaceMembershipsByDocumentId, linkedDocumentsByConversationId,
    loadConversationLinkedDocuments, loadConversationWebSources, loadConversations,
    loadDocumentSpaceMemberships, loadMessageBookmarks, loadSpaces, messageBookmarkMap,
    messageBookmarks, messageMetadata.retrieval, messageMetadata.sources,
    messageMetadata.turn, messageMetadata.verification, messagesQuery.isLoading,
    moveConversationToSpace, queryError?.message, regenerateResponse, retryFailedMessage,
    removeConversationLinkedDocument,
    removeConversationWebSource, renameConversation, selectConversation, sendMessage,
    setComposerDraft, setConversationArchived, setConversationBookmarked, setConversationPinned,
    setConversationSaved, setDocumentSpaceMembership, setFilterMode, setSearchQuery,
    setSelectedSpace, spacesQuery.data, truncateAfter, forkConversation,
    ui.activeConversationId, ui.composerDraft, ui.error, ui.filterMode,
    ui.inFlightGenerations, ui.liveRetrieval, ui.liveSteps, ui.optimisticMessages, ui.searchQuery,
    ui.selectedSpaceId, unbookmarkMessage, webSourcesByConversationId,
  ]);
}
