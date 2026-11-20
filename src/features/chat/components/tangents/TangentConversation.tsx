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
import type { Conversation, ConversationMessage, ConversationMessageBookmark } from '@/types/conversation';

const EMPTY_MESSAGES: ConversationMessage[] = [];
const NO_BOOKMARKS: ConversationMessageBookmark[] = [];

/**
 * Projects a single conversation into a nested `<ChatPanel />` subtree without
 * mutating the global `/chat` `activeConversationId`. Every action, model
 * selection, and streaming state still belongs to the single `ConversationsProvider`
 * above it, while messages, bookmarks, drafts, and active ID are scoped to `id`.
 */
export function ScopedConversationProvider({
  conversationId: id,
  inheritedCount = 0,
  onTurnEnd,
  loadingLabel = 'Opening conversation…',
  errorLabel = 'Couldn’t open this conversation.',
  children,
}: {
  conversationId: string;
  /** How many leading messages a tangent inherited; its own thread starts after them. */
  inheritedCount?: number;
  /** Called each time a turn in this conversation has ended. */
  onTurnEnd?: () => void;
  loadingLabel?: string;
  errorLabel?: string;
  children: ReactNode;
}) {
  const api = useApi();
  const spaces = useConversationsStore((state) => state.spaces);
  const selectedSpaceId = useConversationsStore((state) => state.selectedSpaceId);
  const filterMode = useConversationsStore((state) => state.filterMode);
  const searchQuery = useConversationsStore((state) => state.searchQuery);
  const isLoading = useConversationsStore((state) => state.isLoading);
  const isLoadingBookmarks = useConversationsStore((state) => state.isLoadingBookmarks);
  const inFlightGenerations = useConversationsStore((state) => state.inFlightGenerations);
  const error = useConversationsStore((state) => state.error);
  const optimisticMessages = useConversationsStore((state) => state.optimisticMessages);
  const liveRetrieval = useConversationsStore((state) => state.liveRetrieval);
  const liveSteps = useConversationsStore((state) => state.liveSteps);
  const liveModel = useConversationsStore((state) => state.liveModel);
  const linkedDocumentsByConversationId = useConversationsStore(
    (state) => state.linkedDocumentsByConversationId,
  );
  const webSourcesByConversationId = useConversationsStore(
    (state) => state.webSourcesByConversationId,
  );
  const documentSpaceMembershipsByDocumentId = useConversationsStore(
    (state) => state.documentSpaceMembershipsByDocumentId,
  );
  const loadSpaces = useConversationsStore((state) => state.loadSpaces);
  const setSelectedSpace = useConversationsStore((state) => state.setSelectedSpace);
  const setFilterMode = useConversationsStore((state) => state.setFilterMode);
  const setSearchQuery = useConversationsStore((state) => state.setSearchQuery);
  const loadConversations = useConversationsStore((state) => state.loadConversations);
  const loadMessageBookmarks = useConversationsStore((state) => state.loadMessageBookmarks);
  const createConversation = useConversationsStore((state) => state.createConversation);
  const setConversationModelChoice = useConversationsStore((state) => state.setConversationModelChoice);
  const setConversationSaved = useConversationsStore((state) => state.setConversationSaved);
  const setConversationBookmarked = useConversationsStore((state) => state.setConversationBookmarked);
  const setConversationPinned = useConversationsStore((state) => state.setConversationPinned);
  const setConversationArchived = useConversationsStore((state) => state.setConversationArchived);
  const renameConversation = useConversationsStore((state) => state.renameConversation);
  const bookmarkMessage = useConversationsStore((state) => state.bookmarkMessage);
  const unbookmarkMessage = useConversationsStore((state) => state.unbookmarkMessage);
  const moveConversationToSpace = useConversationsStore((state) => state.moveConversationToSpace);
  const loadConversationLinkedDocuments = useConversationsStore(
    (state) => state.loadConversationLinkedDocuments,
  );
  const loadConversationWebSources = useConversationsStore(
    (state) => state.loadConversationWebSources,
  );
  const addConversationWebSource = useConversationsStore((state) => state.addConversationWebSource);
  const removeConversationWebSource = useConversationsStore((state) => state.removeConversationWebSource);
  const removeConversationLinkedDocument = useConversationsStore(
    (state) => state.removeConversationLinkedDocument,
  );
  const loadDocumentSpaceMemberships = useConversationsStore(
    (state) => state.loadDocumentSpaceMemberships,
  );
  const setDocumentSpaceMembership = useConversationsStore((state) => state.setDocumentSpaceMembership);
  const selectConversation = useConversationsStore((state) => state.selectConversation);
  const sendMessage = useConversationsStore((state) => state.sendMessage);
  const retryFailedMessage = useConversationsStore((state) => state.retryFailedMessage);
  const baseRegenerateResponse = useConversationsStore((state) => state.regenerateResponse);
  const truncateAfter = useConversationsStore((state) => state.truncateAfter);
  const forkConversation = useConversationsStore((state) => state.forkConversation);
  const continueInNewConversation = useConversationsStore((state) => state.continueInNewConversation);
  const compactConversation = useConversationsStore((state) => state.compactConversation);
  const cancelGeneration = useConversationsStore((state) => state.cancelGeneration);
  const deleteMessage = useConversationsStore((state) => state.deleteMessage);
  const deleteConversation = useConversationsStore((state) => state.deleteConversation);
  const dismissFailedMessage = useConversationsStore((state) => state.dismissFailedMessage);
  const clearError = useConversationsStore((state) => state.clearError);
  const messagesQuery = useQuery({ queryKey: conversationKeys.messages(id), queryFn: () => fetchMessages(id, api) });
  const detailQuery = useQuery({ queryKey: conversationKeys.detail(id), queryFn: () => fetchConversationDetail(id, api) });
  const bookmarksQuery = useQuery({ queryKey: conversationKeys.bookmarks(id), queryFn: () => fetchBookmarks(id, undefined, api) });
  const persisted = messagesQuery.data ?? EMPTY_MESSAGES;
  const bookmarks = bookmarksQuery.data ?? NO_BOOKMARKS;
  const metadata = useMemo(() => deriveMessageMetadata(persisted), [persisted]);
  const bookmarkMap = useMemo(() => new Map(bookmarks.map((bookmark) => [bookmark.messageId, bookmark])), [bookmarks]);
  const conversations = useMemo<Conversation[]>(
    () => (detailQuery.data ? [{ ...detailQuery.data, messages: persisted.slice(inheritedCount) }] : []),
    [detailQuery.data, inheritedCount, persisted],
  );
  const sending = inFlightGenerations.has(id);
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
      spaces,
      selectedSpaceId,
      filterMode,
      searchQuery,
      isLoading,
      isLoadingBookmarks,
      inFlightGenerations,
      error,
      optimisticMessages,
      liveRetrieval,
      liveSteps,
      liveModel,
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
      setConversationModelChoice,
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
      truncateAfter,
      forkConversation,
      continueInNewConversation,
      compactConversation,
      cancelGeneration,
      deleteMessage,
      deleteConversation,
      dismissFailedMessage,
      clearError,
      activeConversationId: id,
      conversations,
      lastMessageSources: metadata.sources,
      messageVerification: metadata.verification,
      messageRetrieval: metadata.retrieval,
      messageTurn: metadata.turn,
      messageBookmarks: bookmarks,
      messageBookmarkMap: bookmarkMap,
      composerDraft: null,
      setComposerDraft: (value: string | null) => setDraft(draftKey, value ?? ''),
      regenerateResponse: async (conversationId: string, options?: RegenerateOptions) => {
        const outcome = await baseRegenerateResponse(conversationId, options);
        if (outcome === 'failed') setDraft(draftKey, [...persisted].reverse().find((message) => message.role === 'user')?.content ?? '');
        return outcome;
      },
    };
  }, [
    addConversationWebSource,
    baseRegenerateResponse,
    bookmarkMap,
    bookmarkMessage,
    bookmarks,
    cancelGeneration,
    clearError,
    compactConversation,
    continueInNewConversation,
    conversations,
    createConversation,
    deleteConversation,
    deleteMessage,
    dismissFailedMessage,
    documentSpaceMembershipsByDocumentId,
    error,
    filterMode,
    forkConversation,
    id,
    inFlightGenerations,
    isLoading,
    isLoadingBookmarks,
    linkedDocumentsByConversationId,
    liveModel,
    liveRetrieval,
    liveSteps,
    loadConversationLinkedDocuments,
    loadConversationWebSources,
    loadConversations,
    loadDocumentSpaceMemberships,
    loadMessageBookmarks,
    loadSpaces,
    metadata,
    moveConversationToSpace,
    optimisticMessages,
    persisted,
    removeConversationLinkedDocument,
    removeConversationWebSource,
    renameConversation,
    retryFailedMessage,
    searchQuery,
    selectConversation,
    selectedSpaceId,
    sendMessage,
    setConversationArchived,
    setConversationBookmarked,
    setConversationModelChoice,
    setConversationPinned,
    setConversationSaved,
    setDocumentSpaceMembership,
    setFilterMode,
    setSearchQuery,
    setSelectedSpace,
    spaces,
    truncateAfter,
    unbookmarkMessage,
    webSourcesByConversationId,
  ]);

  if (messagesQuery.data && detailQuery.data) {
    return <ConversationSnapshotProvider value={scope} threadStartsAfter={persisted[inheritedCount - 1]?.id}>{children}</ConversationSnapshotProvider>;
  }
  if (messagesQuery.isError || detailQuery.isError || (detailQuery.isSuccess && !detailQuery.data)) return (
    <div role="alert" className="p-4 text-sm">
      <p>{errorLabel}</p>
      <button type="button" className="mt-2 text-accent underline" onClick={() => { void messagesQuery.refetch(); void detailQuery.refetch(); }}>Try again</button>
    </div>
  );
  return <p role="status" className="p-4 text-sm text-text-muted">{loadingLabel}</p>;
}

/**
 * A tangent's conversation, for the shared chat inside it. Only the view of
 * the controller changes: the chat reads this conversation without selecting
 * it, and every action and all streaming state still belong to the one
 * ConversationsProvider above both panels.
 */
export function TangentConversation({ conversationId, inheritedCount, onTurnEnd, children }: {
  conversationId: string;
  /** How many leading messages the tangent inherited; its own thread starts after them. */
  inheritedCount: number;
  /** Called each time a turn in this tangent has ended. */
  onTurnEnd?: () => void;
  children: ReactNode;
}) {
  return (
    <ScopedConversationProvider
      conversationId={conversationId}
      inheritedCount={inheritedCount}
      onTurnEnd={onTurnEnd}
      loadingLabel="Opening tangent…"
      errorLabel="Couldn’t open this tangent."
    >
      {children}
    </ScopedConversationProvider>
  );
}
