import { useCallback, useMemo } from 'react';

import { useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';

import { VaultAPI } from '@/lib/api';
import type { ChatResponse, ChatStreamEventDto, MessageDto } from '@/lib/bindings';
import type {
  ConversationsState,
  GenerationOutcome,
  LoadConversationsOverrides,
  LoadMessageBookmarksOverrides,
} from '@/stores/conversationsStore.types';
import { conversationUiStore, useConversationUiStore } from '@/stores/conversationUiStore';
import type {
  ApiResult,
  ConversationLinkedDocumentDto,
  ConversationSpaceDto,
  ConversationWebSourceDto,
  DocumentSpaceMembershipDto,
} from '@/types';
import { ErrorCode } from '@/types/api/errorCodes';
import type {
  CompactionRecord,
  Conversation,
  ConversationMessage,
  ConversationMessageBookmark,
  MessageVerificationSummary,
  OptimisticMessage,
  RetrievalTrace,
  SourceWithMetadata,
  ToolPreferences,
} from '@/types/conversation';
import {
  MessageVerificationSummarySchema,
  RetrievalTraceSchema,
  SourceWithMetadataSchema,
  SourcesArraySchema,
} from '@/types/conversation';
import { resolveChatModel } from '@/utils/chatModelSelection';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';

import { conversationKeys, type ConversationListParams } from './queries/conversationKeys';

export { conversationKeys } from './queries/conversationKeys';

const pendingCancellationRequests = new Set<string>();

const setUiError = (error: unknown): void => {
  conversationUiStore.setState({
    error: error instanceof Error ? error.message : String(error),
  });
};

const unwrap = <T,>(result: ApiResult<T>): T => {
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
};

const fetchSpaces = async (): Promise<ConversationSpaceDto[]> =>
  unwrap(await VaultAPI.listConversationSpaces());

const fetchConversationList = async (params: ConversationListParams): Promise<Conversation[]> => {
  const trimmedQuery = params.searchQuery.trim();
  const requiresExplorerQuery = Boolean(
    params.spaceId ||
    trimmedQuery ||
    params.filterMode !== 'all'
  );
  const explorerResult = await VaultAPI.listConversationsExplorer({
    spaceId: params.spaceId ?? undefined,
    query: trimmedQuery || undefined,
    savedOnly: params.filterMode === 'saved' || undefined,
    bookmarkedOnly: params.filterMode === 'bookmarked' || undefined,
    pinnedOnly: params.filterMode === 'pinned' || undefined,
    hasMessageBookmarks: params.filterMode === 'snippets' || undefined,
    includeArchived: params.filterMode === 'archived',
  });

  if (!explorerResult.ok && requiresExplorerQuery) {
    throw new Error(explorerResult.error);
  }

  const result = explorerResult.ok ? explorerResult : await VaultAPI.listConversations();
  if (!result.ok) {
    throw new Error(result.error);
  }
  const conversations = result.data.conversations as Conversation[];
  return params.filterMode === 'archived'
    ? conversations.filter(conversation => Boolean(conversation.isArchived))
    : conversations;
};

const fetchConversationDetail = async (id: string): Promise<Conversation | null> => {
  const result = await VaultAPI.getConversation(id);
  if (!result.ok) throw new Error(result.error);
  return result.data.conversation as Conversation | null;
};

function toConversationMessage(message: MessageDto): ConversationMessage {
  switch (message.status) {
    case 'pending': case 'processing': case 'completed': case 'failed':
      return { ...message, status: message.status };
    default:
      throw new Error(`Unknown conversation message status: ${message.status}`);
  }
}

const fetchMessages = async (id: string): Promise<ConversationMessage[]> => {
  const result = await VaultAPI.getConversationMessages(id);
  if (!result.ok) throw new Error(result.error);
  return result.data.messages.map(toConversationMessage);
};

const fetchBookmarks = async (id: string, query = ''): Promise<ConversationMessageBookmark[]> => {
  const result = await VaultAPI.listMessageBookmarks({
    conversationId: id,
    query: query.trim() || undefined,
    limit: 200,
    offset: 0,
  });
  if (!result.ok) throw new Error(result.error);
  return result.data.bookmarks as ConversationMessageBookmark[];
};

const fetchLinkedDocuments = async (id: string): Promise<ConversationLinkedDocumentDto[]> =>
  unwrap(await VaultAPI.listConversationLinkedDocuments(id));

const fetchWebSources = async (id: string): Promise<ConversationWebSourceDto[]> =>
  unwrap(await VaultAPI.listConversationWebSources(id));

const fetchMemberships = async (documentId: string): Promise<DocumentSpaceMembershipDto[]> =>
  unwrap(await VaultAPI.listDocumentSpaceMemberships(documentId));

const addRequestedId = (
  field:
    | 'requestedLinkedConversationIds'
    | 'requestedWebSourceConversationIds'
    | 'requestedMembershipDocumentIds',
  id: string
): void => {
  conversationUiStore.setState(state => {
    const next = new Set(state[field]);
    next.add(id);
    return { [field]: next };
  });
};

const normalizeChunkExcerpt = (raw: unknown): Record<string, unknown> | null => {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const chunkId = String(value.chunkId ?? value.chunk_id ?? '').trim();
  const excerpt = String(value.excerpt ?? '').trim();
  if (!chunkId || !excerpt) return null;
  const rawScore = Number(value.score ?? 0);
  return {
    chunkId,
    excerpt,
    section: value.section ?? undefined,
    chunkIndex: value.chunkIndex ?? value.chunk_index ?? undefined,
    pageNumber: value.pageNumber ?? value.page_number ?? undefined,
    score: Number.isFinite(rawScore) && rawScore >= 0 ? rawScore : 0,
    highlights: value.highlights ?? undefined,
  };
};

const normalizeSource = (raw: unknown): Record<string, unknown> | null => {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const filePath = String(value.filePath ?? value.file_path ?? '').trim();
  const fileName = String(value.fileName ?? value.file_name ?? '').trim();
  const documentId = String(value.documentId ?? value.document_id ?? '').trim();
  const chunkId = String(value.chunkId ?? value.chunk_id ?? '').trim();
  const resolvedPath = filePath || 'unknown://source';
  const resolvedDocumentId = documentId || `source:${resolvedPath}`;
  const rawScore = Number(value.score ?? 0);
  const rawExcerpts = Array.isArray(value.chunkExcerpts)
    ? value.chunkExcerpts
    : (Array.isArray(value.chunk_excerpts) ? value.chunk_excerpts : []);
  return {
    documentId: resolvedDocumentId,
    chunkId: chunkId || `${resolvedDocumentId}#chunk`,
    fileName: fileName || resolvedPath,
    filePath: resolvedPath,
    mimeType: value.mimeType ?? value.mime_type ?? 'text/plain',
    category: value.category ?? (resolvedPath.startsWith('http') ? 'Web Article' : 'Unknown'),
    content: String(value.content ?? '').trim(),
    excerpt: value.excerpt ?? undefined,
    highlights: value.highlights ?? undefined,
    section: value.section ?? undefined,
    chunkIndex: value.chunkIndex ?? value.chunk_index ?? undefined,
    pageNumber: value.pageNumber ?? value.page_number ?? undefined,
    chunkExcerpts: rawExcerpts
      .map(normalizeChunkExcerpt)
      .filter((item): item is Record<string, unknown> => item !== null),
    score: Number.isFinite(rawScore) && rawScore >= 0 ? rawScore : 0,
    fileSizeBytes: value.fileSizeBytes ?? value.file_size_bytes ?? 0,
    modifiedAt: value.modifiedAt ?? value.modified_at ?? '',
    citationId: value.citationId ?? value.citation_id ?? undefined,
  };
};

const parseSources = (raw: unknown): SourceWithMetadata[] => {
  if (!Array.isArray(raw)) return [];
  const normalized = raw
    .map(normalizeSource)
    .filter((source): source is Record<string, unknown> => source !== null);
  const parsedArray = SourcesArraySchema.safeParse(normalized);
  if (parsedArray.success) return parsedArray.data;
  return normalized.flatMap(source => {
    const parsed = SourceWithMetadataSchema.safeParse(source);
    return parsed.success ? [parsed.data] : [];
  });
};

const parseVerification = (raw: unknown): MessageVerificationSummary | null => {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const parsed = MessageVerificationSummarySchema.safeParse({
    enabled: Boolean(value.enabled),
    claimsEvaluated: value.claimsEvaluated ?? value.claims_evaluated,
    supportedClaims: value.supportedClaims ?? value.supported_claims,
    supportedClaimNotes: value.supportedClaimNotes ?? value.supported_claim_notes,
    unsupportedClaims: value.unsupportedClaims ?? value.unsupported_claims,
    groundedRatio: value.groundedRatio ?? value.grounded_ratio,
    contradictedClaims: value.contradictedClaims ?? value.contradicted_claims,
    verdictCounts: value.verdictCounts ?? value.verdict_counts,
    claimVerdicts: value.claimVerdicts ?? value.claim_verdicts,
    judgeUsed: value.judgeUsed ?? value.judge_used,
  });
  return parsed.success ? parsed.data : null;
};

/**
 * A persisted retrieval trace, or null.
 *
 * Absent means "no trace" — messages written before this feature, and turns
 * where retrieval never ran, have no key. Never read that as zeros.
 */
const parseRetrievalTrace = (raw: unknown): RetrievalTrace | null => {
  if (!raw || typeof raw !== 'object') return null;
  const parsed = RetrievalTraceSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
};

const deriveMessageMetadata = (messages: ConversationMessage[]) => {
  const sources = new Map<string, SourceWithMetadata[]>();
  const verification = new Map<string, MessageVerificationSummary>();
  const retrieval = new Map<string, RetrievalTrace>();
  for (const message of messages) {
    const directSources = parseSources(message.sources);
    if (directSources.length > 0) sources.set(message.id, directSources);
    if (!message.metadata) continue;
    try {
      const metadata = JSON.parse(message.metadata) as Record<string, unknown>;
      const persistedSources = parseSources(metadata.sources);
      if (persistedSources.length > 0) sources.set(message.id, persistedSources);
      const persistedVerification = parseVerification(metadata.verification);
      if (persistedVerification) verification.set(message.id, persistedVerification);
      const persistedRetrieval = parseRetrievalTrace(metadata.retrieval);
      if (persistedRetrieval) retrieval.set(message.id, persistedRetrieval);
    } catch {
      // A malformed metadata field must not hide the message itself.
    }
  }
  return { sources, verification, retrieval };
};

const isUserInitiatedCancellation = (requestId: string, errorCode?: string): boolean =>
  pendingCancellationRequests.has(requestId) &&
  (errorCode === ErrorCode.INVALID_STATE || errorCode === ErrorCode.INTERNAL_ERROR);

export function useConversationsController(): ConversationsState {
  const queryClient = useQueryClient();
  const ui = useConversationUiStore();
  const listParams = useMemo<ConversationListParams>(() => ({
    spaceId: ui.selectedSpaceId,
    filterMode: ui.filterMode,
    searchQuery: ui.searchQuery,
  }), [ui.filterMode, ui.searchQuery, ui.selectedSpaceId]);

  const spacesQuery = useQuery({
    queryKey: conversationKeys.spaces,
    queryFn: fetchSpaces,
    staleTime: 30_000,
  });
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
    if (activeId && detailQuery.data && !list.some(item => item.id === activeId)) {
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

  const invalidateLists = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
  }, [queryClient]);

  const loadSpaces = useCallback(async () => {
    try {
      await queryClient.fetchQuery({ queryKey: conversationKeys.spaces, queryFn: fetchSpaces });
    } catch (error) {
      setUiError(error);
    }
  }, [queryClient]);

  const setSelectedSpace = useCallback((spaceId: string | null) => {
    conversationUiStore.setState({ selectedSpaceId: spaceId });
  }, []);
  const setFilterMode = useCallback((filterMode: ConversationsState['filterMode']) => {
    conversationUiStore.setState({ filterMode });
  }, []);
  const setSearchQuery = useCallback((searchQuery: string) => {
    conversationUiStore.setState({ searchQuery });
  }, []);

  const loadConversations = useCallback(async (overrides?: LoadConversationsOverrides) => {
    const current = conversationUiStore.getState();
    const params: ConversationListParams = {
      spaceId: overrides?.spaceId !== undefined ? overrides.spaceId : current.selectedSpaceId,
      filterMode: overrides?.filterMode ?? current.filterMode,
      searchQuery: overrides?.searchQuery ?? current.searchQuery,
    };
    conversationUiStore.setState({
      selectedSpaceId: params.spaceId,
      filterMode: params.filterMode,
      searchQuery: params.searchQuery,
      error: null,
    });
    try {
      await queryClient.fetchQuery({
        queryKey: conversationKeys.list(params),
        queryFn: () => fetchConversationList(params),
        staleTime: 0,
      });
    } catch (error) {
      setUiError(error);
    }
  }, [queryClient]);

  const loadMessageBookmarks = useCallback(async (overrides?: LoadMessageBookmarksOverrides) => {
    const id = overrides?.conversationId ?? conversationUiStore.getState().activeConversationId;
    if (!id) return;
    const query = overrides?.query?.trim() ?? '';
    try {
      await queryClient.fetchQuery({
        queryKey: conversationKeys.bookmarks(id, query),
        queryFn: () => fetchBookmarks(id, query),
        staleTime: 0,
      });
    } catch (error) {
      setUiError(error);
    }
  }, [queryClient]);

  const createConversation = useCallback(async (title: string): Promise<string> => {
    const state = conversationUiStore.getState();
    const spaces = await queryClient.ensureQueryData({
      queryKey: conversationKeys.spaces,
      queryFn: fetchSpaces,
    });
    const selectedSpace = spaces.find(space => space.id === state.selectedSpaceId);
    if (state.selectedSpaceId && !selectedSpace) {
      const error = new Error('The selected space is unavailable. Select a space and try again.');
      setUiError(error);
      throw error;
    }
    const [activeModelsResult, settingsResult] = await Promise.all([
      VaultAPI.getActiveModels(),
      VaultAPI.getSettings(),
    ]);
    const activeModel = activeModelsResult.ok ? activeModelsResult.data.chat_model : null;
    const llmSettings = settingsResult.ok ? settingsResult.data.llm : null;
    const spaceModel = selectedSpace?.defaultModelName?.trim();
    const modelName = spaceModel || resolveChatModel(llmSettings, activeModel?.model_id ?? null);
    if (!modelName) {
      const error = new Error('No chat model available. Select a chat provider and model in Settings.');
      setUiError(error);
      throw error;
    }

    const result = await VaultAPI.createConversation(title, modelName);
    if (!result.ok) {
      const error = new Error(result.error);
      setUiError(error);
      throw error;
    }
    const id = result.data.conversation.id;
    let created = result.data.conversation as Conversation;
    if (state.selectedSpaceId && state.selectedSpaceId !== 'space_general' && selectedSpace) {
      const moveResult = await VaultAPI.moveConversationToSpace({
        conversationId: id,
        spaceId: state.selectedSpaceId,
      });
      if (!moveResult.ok) {
        const error = new Error(`Could not create the chat in ${selectedSpace.name}: ${moveResult.error}`);
        setUiError(error);
        await invalidateLists();
        throw error;
      }
      // The create response still describes General. Read the saved destination
      // before opening the chat or letting a message use its scope.
      const moved = await fetchConversationDetail(id);
      if (moved?.spaceId !== state.selectedSpaceId) {
        const error = new Error('Could not confirm the new chat’s space. Refresh and try again.');
        setUiError(error);
        throw error;
      }
      created = moved;
    }
    queryClient.setQueryData(conversationKeys.detail(id), created);
    queryClient.setQueryData(conversationKeys.messages(id), []);
    conversationUiStore.setState({ activeConversationId: id });
    await invalidateLists();
    return id;
  }, [invalidateLists, queryClient]);

  const setConversationFlag = useCallback(async (
    id: string,
    value: boolean,
    operation: typeof VaultAPI.setConversationSaved
  ) => {
    const result = await operation({ conversationId: id, value });
    if (!result.ok) {
      setUiError(result.error);
      return;
    }
    await invalidateLists();
  }, [invalidateLists]);

  const setConversationSaved = useCallback(
    (id: string, value: boolean) => setConversationFlag(id, value, VaultAPI.setConversationSaved),
    [setConversationFlag]
  );
  const setConversationBookmarked = useCallback(
    (id: string, value: boolean) => setConversationFlag(id, value, VaultAPI.setConversationBookmarked),
    [setConversationFlag]
  );
  const setConversationPinned = useCallback(
    (id: string, value: boolean) => setConversationFlag(id, value, VaultAPI.setConversationPinned),
    [setConversationFlag]
  );
  const setConversationArchived = useCallback(async (id: string, value: boolean) => {
    await setConversationFlag(id, value, VaultAPI.setConversationArchived);
    if (value && conversationUiStore.getState().activeConversationId === id) {
      conversationUiStore.setState({ activeConversationId: null });
    }
  }, [setConversationFlag]);

  const renameConversation = useCallback(async (id: string, title: string): Promise<boolean> => {
    const nextTitle = title.trim();
    if (!nextTitle) {
      setUiError('Conversation title cannot be empty.');
      return false;
    }
    const result = await VaultAPI.renameConversation(id, nextTitle);
    if (!result.ok) {
      setUiError(result.error);
      return false;
    }
    queryClient.setQueriesData<Conversation[]>({ queryKey: conversationKeys.lists }, current =>
      current?.map(item => item.id === id
        ? { ...item, title: nextTitle, updatedAt: new Date().toISOString() }
        : item)
    );
    queryClient.setQueryData<Conversation | null>(conversationKeys.detail(id), current =>
      current ? { ...current, title: nextTitle, updatedAt: new Date().toISOString() } : current
    );
    await queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks });
    return true;
  }, [queryClient]);

  const bookmarkMessage = useCallback(async (
    conversationId: string,
    messageId: string,
    title?: string | null,
    note?: string | null
  ) => {
    const result = await VaultAPI.bookmarkConversationMessage({ conversationId, messageId, title, note });
    if (!result.ok) setUiError(result.error);
    else await queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks });
  }, [queryClient]);

  const unbookmarkMessage = useCallback(async (conversationId: string, messageId: string) => {
    const result = await VaultAPI.unbookmarkConversationMessage({ conversationId, messageId });
    if (!result.ok) setUiError(result.error);
    else await queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks });
  }, [queryClient]);

  const moveConversationToSpace = useCallback(async (id: string, spaceId: string) => {
    const result = await VaultAPI.moveConversationToSpace({ conversationId: id, spaceId });
    if (!result.ok) setUiError(result.error);
    else await Promise.all([invalidateLists(), queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks })]);
  }, [invalidateLists, queryClient]);

  const loadConversationLinkedDocuments = useCallback(async (conversationId: string) => {
    addRequestedId('requestedLinkedConversationIds', conversationId);
    try {
      await queryClient.fetchQuery({
        queryKey: conversationKeys.linkedDocuments(conversationId),
        queryFn: () => fetchLinkedDocuments(conversationId),
        staleTime: 0,
      });
    } catch (error) {
      setUiError(error);
    }
  }, [queryClient]);

  const loadConversationWebSources = useCallback(async (conversationId: string) => {
    addRequestedId('requestedWebSourceConversationIds', conversationId);
    try {
      await queryClient.fetchQuery({
        queryKey: conversationKeys.webSources(conversationId),
        queryFn: () => fetchWebSources(conversationId),
        staleTime: 0,
      });
    } catch (error) {
      setUiError(error);
    }
  }, [queryClient]);

  const addConversationWebSource = useCallback(async (
    conversationId: string,
    url: string,
    options?: { title?: string; excerpt?: string; relevanceScore?: number }
  ): Promise<boolean> => {
    const result = await VaultAPI.addConversationWebSource(conversationId, url, options);
    if (!result.ok) {
      setUiError(result.error);
      return false;
    }
    await loadConversationWebSources(conversationId);
    return true;
  }, [loadConversationWebSources]);

  const removeConversationWebSource = useCallback(async (
    conversationId: string,
    sourceId: string
  ): Promise<boolean> => {
    const result = await VaultAPI.removeConversationWebSource(conversationId, sourceId);
    if (!result.ok) {
      setUiError(result.error);
      return false;
    }
    queryClient.setQueryData<ConversationWebSourceDto[]>(
      conversationKeys.webSources(conversationId),
      current => current?.filter(source => source.id !== sourceId)
    );
    return true;
  }, [queryClient]);

  const removeConversationLinkedDocument = useCallback(async (
    conversationId: string,
    documentId: string
  ) => {
    const result = await VaultAPI.removeConversationLinkedDocument(conversationId, documentId);
    if (!result.ok) {
      setUiError(result.error);
      return;
    }
    queryClient.setQueryData<ConversationLinkedDocumentDto[]>(
      conversationKeys.linkedDocuments(conversationId),
      current => current?.filter(document => document.documentId !== documentId)
    );
    queryClient.removeQueries({ queryKey: conversationKeys.memberships(documentId), exact: true });
  }, [queryClient]);

  const loadDocumentSpaceMemberships = useCallback(async (documentId: string) => {
    addRequestedId('requestedMembershipDocumentIds', documentId);
    try {
      await queryClient.fetchQuery({
        queryKey: conversationKeys.memberships(documentId),
        queryFn: () => fetchMemberships(documentId),
        staleTime: 0,
      });
    } catch (error) {
      setUiError(error);
    }
  }, [queryClient]);

  const setDocumentSpaceMembership = useCallback(async (
    documentId: string,
    spaceId: string,
    assigned: boolean
  ) => {
    const result = await VaultAPI.setDocumentSpaceMembership(documentId, spaceId, assigned);
    if (!result.ok) setUiError(result.error);
    else await loadDocumentSpaceMemberships(documentId);
  }, [loadDocumentSpaceMemberships]);

  const selectConversation = useCallback(async (id: string) => {
    conversationUiStore.setState({ activeConversationId: id, error: null });
    addRequestedId('requestedLinkedConversationIds', id);
    addRequestedId('requestedWebSourceConversationIds', id);
    try {
      await Promise.all([
        queryClient.fetchQuery({ queryKey: conversationKeys.messages(id), queryFn: () => fetchMessages(id), staleTime: 0 }),
        queryClient.fetchQuery({ queryKey: conversationKeys.detail(id), queryFn: () => fetchConversationDetail(id), staleTime: 0 }),
        queryClient.fetchQuery({ queryKey: conversationKeys.bookmarks(id), queryFn: () => fetchBookmarks(id), staleTime: 0 }),
        queryClient.fetchQuery({ queryKey: conversationKeys.linkedDocuments(id), queryFn: () => fetchLinkedDocuments(id), staleTime: 0 }),
        queryClient.fetchQuery({ queryKey: conversationKeys.webSources(id), queryFn: () => fetchWebSources(id), staleTime: 0 }),
      ]);
    } catch (error) {
      setUiError(error);
    }
  }, [queryClient]);

  /**
   * One generation turn, shared by `sendMessage` and `regenerateResponse`.
   *
   * The only difference between the two is whether a user bubble is pushed:
   * regenerate re-runs a question that is already on screen and about to be
   * re-persisted server-side, so pushing one would show it twice. Everything
   * else — the `llm-stream` listener, the retrieval trace, the optimistic
   * assistant bubble, the settle path — is identical, and both refetch from
   * the server on success so the backend stays the single source of truth.
   *
   * Returns how the turn ended. `onFailure` fires only on a real failure, so it
   * is not a complete signal on its own: the busy guard and a user-initiated
   * cancellation both end the turn without it.
   */
  const runGeneration = useCallback(async (options: {
    conversationId: string;
    content: string;
    showUserBubble: boolean;
    invoke: (_requestId: string) => Promise<ApiResult<ChatResponse>>;
    onFailure?: (_error: string) => void;
  }): Promise<GenerationOutcome> => {
    const { conversationId: requestConversationId, content, showUserBubble, invoke, onFailure } = options;
    const state = conversationUiStore.getState();
    if (state.inFlightGenerations.has(requestConversationId)) {
      setUiError('A response is already being generated for this conversation.');
      return 'busy';
    }

    const requestId = crypto.randomUUID();
    const userTempId = crypto.randomUUID();
    const assistantTempId = crypto.randomUUID();
    const now = new Date().toISOString();
    const userMessage: OptimisticMessage = {
      tempId: userTempId,
      conversationId: requestConversationId,
      content,
      role: 'user',
      status: 'pending',
      createdAt: now,
    };
    const assistantMessage: OptimisticMessage = {
      tempId: assistantTempId,
      conversationId: requestConversationId,
      content: '',
      role: 'assistant',
      status: 'pending',
      createdAt: now,
    };
    conversationUiStore.setState(current => {
      const optimisticMessages = new Map(current.optimisticMessages);
      if (showUserBubble) optimisticMessages.set(userTempId, userMessage);
      optimisticMessages.set(assistantTempId, assistantMessage);
      const inFlightGenerations = new Map(current.inFlightGenerations);
      inFlightGenerations.set(requestConversationId, requestId);
      const liveRetrieval = new Map(current.liveRetrieval);
      liveRetrieval.delete(requestConversationId);
      return { optimisticMessages, inFlightGenerations, liveRetrieval, error: null };
    });

    const settleOptimisticMessages = (error?: string) => {
      conversationUiStore.setState(current => {
        const optimisticMessages = new Map(current.optimisticMessages);
        if (error && showUserBubble) {
          const failed = optimisticMessages.get(userTempId);
          if (failed) {
            optimisticMessages.set(userTempId, { ...failed, status: 'failed', error });
          }
        } else {
          optimisticMessages.delete(userTempId);
        }
        optimisticMessages.delete(assistantTempId);
        return {
          optimisticMessages,
          ...(error ? { error } : {}),
        };
      });
      if (error) onFailure?.(error);
    };

    const retryingNotice = 'Model response failed. Retrying...';
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<ChatStreamEventDto>('llm-stream', event => {
        const payload = event.payload;
        if (payload.conversationId !== requestConversationId || payload.requestId !== requestId) return;
        if (payload.done) {
          unlisten?.();
          return;
        }
        // Tool searches can update the initial retrieval trace during generation.
        if (payload.status === 'retrieval' && payload.retrieval) {
          const parsed = RetrievalTraceSchema.safeParse(payload.retrieval);
          if (parsed.success) {
            conversationUiStore.setState(current => {
              const liveRetrieval = new Map(current.liveRetrieval);
              liveRetrieval.set(requestConversationId, parsed.data);
              return { liveRetrieval };
            });
          }
          return;
        }
        const nextChunk = payload.status === 'retrying'
          ? (typeof payload.attempt === 'number'
            ? `${retryingNotice} (attempt ${payload.attempt})`
            : retryingNotice)
          : payload.content;
        if (!nextChunk) return;
        conversationUiStore.setState(current => {
          const optimisticMessages = new Map(current.optimisticMessages);
          const existing = optimisticMessages.get(assistantTempId);
          if (existing) {
            optimisticMessages.set(assistantTempId, {
              ...existing,
              content: payload.status === 'retrying' || existing.content.startsWith(retryingNotice)
                ? nextChunk
                : existing.content + nextChunk,
            });
          }
          return { optimisticMessages };
        });
      });

      const result = await invoke(requestId);
      if (!result.ok) {
        if (isUserInitiatedCancellation(requestId, result.details?.code)) {
          const refreshed = await fetchMessages(requestConversationId);
          queryClient.setQueryData(conversationKeys.messages(requestConversationId), refreshed);
          settleOptimisticMessages();
          conversationUiStore.setState({ error: null });
          return 'cancelled';
        }
        const detail: unknown = result.details?.details;
        settleOptimisticMessages(typeof detail === 'string' && detail.trim() ? detail : result.error);
        return 'failed';
      }

      const raw = result.data as typeof result.data & {
        conversation_id?: string;
        sources?: SourceWithMetadata[];
      };
      const responseId = raw.conversationId ?? raw.conversation_id;
      if (!responseId) {
        settleOptimisticMessages('Chat response missing conversation ID. Please try again.');
        return 'failed';
      }
      const responseMessages = raw.messages.map(toConversationMessage);
      const lastAssistant = [...responseMessages].reverse().find(message => message.role === 'assistant');
      if (lastAssistant && raw.sources?.length && !lastAssistant.sources?.length) {
        lastAssistant.sources = parseSources(raw.sources);
      }
      queryClient.setQueryData(conversationKeys.messages(responseId), responseMessages);
      queryClient.setQueriesData<Conversation[]>({ queryKey: conversationKeys.lists }, current => {
        if (!current) return current;
        const existing = current.find(item => item.id === responseId);
        if (existing) {
          return current.map(item => item.id === responseId
            ? { ...item, updatedAt: new Date().toISOString() }
            : item);
        }
        return [{
          id: responseId,
          title: createDefaultConversationTitle(),
          updatedAt: new Date().toISOString(),
        }, ...current];
      });
      conversationUiStore.setState(current => {
        const optimisticMessages = new Map(current.optimisticMessages);
        optimisticMessages.delete(userTempId);
        optimisticMessages.delete(assistantTempId);
        return { activeConversationId: responseId, optimisticMessages };
      });
      addRequestedId('requestedLinkedConversationIds', responseId);
      await Promise.all([
        invalidateLists(),
        queryClient.invalidateQueries({ queryKey: conversationKeys.linkedDocuments(responseId) }),
      ]);
      return 'answered';
    } catch (error) {
      settleOptimisticMessages(error instanceof Error ? error.message : String(error));
      return 'failed';
    } finally {
      unlisten?.();
      pendingCancellationRequests.delete(requestId);
      conversationUiStore.setState(current => {
        const inFlightGenerations = new Map(current.inFlightGenerations);
        if (inFlightGenerations.get(requestConversationId) === requestId) {
          inFlightGenerations.delete(requestConversationId);
        }
        const liveRetrieval = new Map(current.liveRetrieval);
        liveRetrieval.delete(requestConversationId);
        return { inFlightGenerations, liveRetrieval };
      });
    }
  }, [invalidateLists, queryClient]);

  const sendMessage = useCallback(async (
    content: string,
    conversationId?: string | null,
    toolPreferences?: ToolPreferences
  ) => {
    const state = conversationUiStore.getState();
    const requestConversationId = conversationId ?? state.activeConversationId ?? conversations[0]?.id ?? null;
    if (!requestConversationId) {
      setUiError('No active conversation selected. Create or select a conversation first.');
      return;
    }
    await runGeneration({
      conversationId: requestConversationId,
      content,
      showUserBubble: true,
      invoke: requestId => VaultAPI.chatWithConversation(
        requestConversationId,
        content,
        toolPreferences,
        requestId
      ),
    });
  }, [conversations, runGeneration]);

  /**
   * Re-run the last user message.
   *
   * The backend lifts the question off the thread and re-persists it, so no
   * user bubble is pushed here. If generation fails it hands the question back
   * through `onFailure` so the caller can put it in the composer — a
   * regenerate must never cost the user their question.
   *
   * Returns how the turn ended, so callers announce neither an answer that
   * never arrived nor a failure that never happened.
   */
  const regenerateResponse = useCallback(async (
    conversationId: string,
    toolPreferences?: ToolPreferences
  ): Promise<GenerationOutcome> => {
    const messages = queryClient.getQueryData<ConversationMessage[]>(
      conversationKeys.messages(conversationId)
    ) ?? [];
    const lastUser = [...messages].reverse().find(message => message.role === 'user');
    return runGeneration({
      conversationId,
      content: lastUser?.content ?? '',
      showUserBubble: false,
      invoke: requestId => VaultAPI.regenerateResponse(conversationId, toolPreferences, requestId),
      onFailure: () => {
        if (lastUser?.content) {
          conversationUiStore.setState({ composerDraft: lastUser.content });
        }
      },
    });
  }, [queryClient, runGeneration]);

  /**
   * Drop every message after `messageId` (and it too when `inclusive`).
   * Replaces the message cache with what the server says survived.
   */
  const truncateAfter = useCallback(async (
    conversationId: string,
    messageId: string,
    inclusive = false
  ): Promise<boolean> => {
    const result = await VaultAPI.truncateConversationAfter(conversationId, messageId, inclusive);
    if (!result.ok) {
      setUiError(result.error);
      return false;
    }
    queryClient.setQueryData(
      conversationKeys.messages(conversationId),
      result.data.messages.map(toConversationMessage)
    );
    await invalidateLists();
    return true;
  }, [invalidateLists, queryClient]);

  /** Branch the conversation, select the branch, and return its id. */
  const forkConversation = useCallback(async (
    conversationId: string,
    upToMessageId?: string
  ): Promise<string | null> => {
    const result = await VaultAPI.forkConversation(conversationId, upToMessageId);
    if (!result.ok) {
      setUiError(result.error);
      return null;
    }
    const newId = result.data.conversation.id;
    await invalidateLists();
    await selectConversation(newId);
    return newId;
  }, [invalidateLists, selectConversation]);

  /**
   * Fold the conversation's oldest messages into an LLM summary.
   *
   * The backend keeps the raw messages for display and only switches the LLM
   * context to the summary. On success the lists are refreshed and the applied
   * record is returned so the caller can render a divider; on failure the error
   * is surfaced and `null` is returned.
   */
  const compactConversation = useCallback(async (
    conversationId: string,
    keepRecentMessages?: number
  ): Promise<CompactionRecord | null> => {
    const result = await VaultAPI.compactConversation(conversationId, keepRecentMessages);
    if (!result.ok) {
      setUiError(result.error);
      return null;
    }
    await Promise.all([
      invalidateLists(),
      // The detail read is what carries the compaction back after a reload.
      queryClient.invalidateQueries({
        queryKey: conversationKeys.detail(conversationId),
      }),
    ]);
    return result.data.compaction;
  }, [invalidateLists, queryClient]);

  const cancelGeneration = useCallback(async (conversationId?: string | null) => {
    const state = conversationUiStore.getState();
    const id = conversationId ?? state.activeConversationId;
    if (!id) return;
    const requestId = state.inFlightGenerations.get(id);
    if (!requestId) return;
    pendingCancellationRequests.add(requestId);
    const result = await VaultAPI.cancelConversationGeneration(id, requestId);
    if (!result.ok) {
      pendingCancellationRequests.delete(requestId);
      setUiError(result.error);
    }
  }, []);

  const deleteMessage = useCallback(async (conversationId: string, messageId: string) => {
    const key = conversationKeys.messages(conversationId);
    const previous = queryClient.getQueryData<ConversationMessage[]>(key);
    if (!previous?.some(message => message.id === messageId)) {
      setUiError('Message not found in the selected conversation.');
      return;
    }
    queryClient.setQueryData(key, previous.filter(message => message.id !== messageId));
    const result = await VaultAPI.deleteConversationMessage({ conversationId, messageId });
    if (!result.ok) {
      queryClient.setQueryData(key, previous);
      setUiError(result.error);
      return;
    }
    await Promise.all([
      invalidateLists(),
      queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks }),
    ]);
  }, [invalidateLists, queryClient]);

  const deleteConversation = useCallback(async (id: string) => {
    const snapshots = queryClient.getQueriesData<Conversation[]>({ queryKey: conversationKeys.lists });
    const exists = snapshots.some(([, list]) => list?.some(item => item.id === id));
    if (!exists) {
      setUiError(`Conversation not found: ${id}`);
      return;
    }
    queryClient.setQueriesData<Conversation[]>({ queryKey: conversationKeys.lists }, current =>
      current?.filter(item => item.id !== id)
    );
    const wasActive = conversationUiStore.getState().activeConversationId === id;
    if (wasActive) conversationUiStore.setState({ activeConversationId: null });
    const result = await VaultAPI.deleteConversation(id);
    if (!result.ok) {
      for (const [key, data] of snapshots) queryClient.setQueryData(key, data);
      if (wasActive) conversationUiStore.setState({ activeConversationId: id });
      setUiError(result.error);
      return;
    }
    queryClient.removeQueries({ queryKey: conversationKeys.detail(id), exact: true });
    queryClient.removeQueries({ queryKey: conversationKeys.messages(id), exact: true });
    queryClient.removeQueries({ queryKey: conversationKeys.bookmarks(id) });
    queryClient.removeQueries({ queryKey: conversationKeys.linkedDocuments(id), exact: true });
    queryClient.removeQueries({ queryKey: conversationKeys.webSources(id), exact: true });
    await queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks });
  }, [queryClient]);

  /** Pure UI state: the text the composer should adopt on its next render. */
  const setComposerDraft = useCallback(
    (draft: string | null) => conversationUiStore.setState({ composerDraft: draft }),
    []
  );

  const clearError = useCallback(() => conversationUiStore.setState({ error: null }), []);
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
    liveRetrieval: ui.liveRetrieval,
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
    regenerateResponse,
    truncateAfter,
    forkConversation,
    compactConversation,
    setComposerDraft,
    cancelGeneration,
    deleteMessage,
    deleteConversation,
    clearError,
  }), [
    addConversationWebSource, bookmarkMessage, bookmarksQuery.isLoading, cancelGeneration,
    clearError, compactConversation, conversations, conversationsQuery.isLoading,
    createConversation, deleteConversation,
    deleteMessage, documentSpaceMembershipsByDocumentId, linkedDocumentsByConversationId,
    loadConversationLinkedDocuments, loadConversationWebSources, loadConversations,
    loadDocumentSpaceMemberships, loadMessageBookmarks, loadSpaces, messageBookmarkMap,
    messageBookmarks, messageMetadata.retrieval, messageMetadata.sources,
    messageMetadata.verification, messagesQuery.isLoading,
    moveConversationToSpace, queryError?.message, regenerateResponse,
    removeConversationLinkedDocument,
    removeConversationWebSource, renameConversation, selectConversation, sendMessage,
    setComposerDraft, setConversationArchived, setConversationBookmarked, setConversationPinned,
    setConversationSaved, setDocumentSpaceMembership, setFilterMode, setSearchQuery,
    setSelectedSpace, spacesQuery.data, truncateAfter, forkConversation,
    ui.activeConversationId, ui.composerDraft, ui.error, ui.filterMode,
    ui.inFlightGenerations, ui.liveRetrieval, ui.optimisticMessages, ui.searchQuery,
    ui.selectedSpaceId, unbookmarkMessage, webSourcesByConversationId,
  ]);
}
