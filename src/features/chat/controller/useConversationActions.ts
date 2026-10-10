import { useCallback } from 'react';


import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { saveBookmark, removeBookmark } from '@/features/references/api/queries';
import { settingsQueryOptions } from '@/features/settings/hooks/useSettingsQuery';
import { spacesQueryOptions } from '@/features/spaces/api/queries';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { VaultAPI } from '@/lib/api';
import type { ConversationTangentDto } from '@/lib/bindings';
import { conversationKeys, type ConversationListParams } from '@/shared/conversations/conversationKeys';
import type { CompactionResult, ConversationsState, LoadConversationsOverrides, LoadMessageBookmarksOverrides } from '@/shared/conversations/conversationsStore.types';
import type {
  ConversationLinkedDocumentDto, ConversationWebSourceDto,
} from '@/types';
import type { Conversation, ConversationMessage } from '@/types/conversation';
import { resolveChatModel } from '@/utils/chatModelSelection';

import { fetchBookmarks, fetchConversationDetail, fetchConversationList, fetchLinkedDocuments, fetchMemberships, fetchMessages, fetchWebSources, toConversationMessage } from '../api/conversationQueryData';

import type { ConversationLifecycleRegistry } from './lifecycleRegistry';
import type { QueryClient } from '@tanstack/react-query';

export const MAX_OBSERVED_CONVERSATIONS = 20;
export const MAX_OBSERVED_MEMBERSHIPS = 100;

const setUiError = (error: unknown): void => {
  conversationUiStore.setState({ error: error instanceof Error ? error.message : String(error) });
};

function restoreMissingItem<T extends { id: string }>(current: T[] | undefined, previous: T[], id: string): T[] | undefined {
  const targetIndex = previous.findIndex(item => item.id === id);
  const target = previous[targetIndex];
  if (!target || current?.some(item => item.id === id)) return current;

  const restored = [...(current ?? [])];
  let insertionIndex = -1;
  // Use surviving history as anchors so concurrent deletions do not move the
  // restored item past messages that followed it in the original sequence.
  for (let index = targetIndex + 1; index < previous.length; index += 1) {
    const successorIndex = restored.findIndex(item => item.id === previous[index].id);
    if (successorIndex >= 0) {
      insertionIndex = successorIndex;
      break;
    }
  }
  if (insertionIndex < 0) {
    for (let index = targetIndex - 1; index >= 0; index -= 1) {
      const predecessorIndex = restored.findIndex(item => item.id === previous[index].id);
      if (predecessorIndex >= 0) {
        insertionIndex = predecessorIndex + 1;
        break;
      }
    }
  }
  if (insertionIndex < 0) insertionIndex = Math.min(targetIndex, restored.length);
  restored.splice(insertionIndex, 0, target);
  return restored;
}

const forgetRequestedConversation = (id: string): void => {
  conversationUiStore.setState(state => {
    const linked = new Set(state.requestedLinkedConversationIds);
    const web = new Set(state.requestedWebSourceConversationIds);
    linked.delete(id);
    web.delete(id);
    const optimisticMessages = new Map([...state.optimisticMessages].filter(([, message]) => message.conversationId !== id));
    const inFlightGenerations = new Map(state.inFlightGenerations);
    const liveRetrieval = new Map(state.liveRetrieval);
    const liveSteps = new Map(state.liveSteps);
    inFlightGenerations.delete(id);
    liveRetrieval.delete(id);
    liveSteps.delete(id);
    return { requestedLinkedConversationIds: linked, requestedWebSourceConversationIds: web, optimisticMessages, inFlightGenerations, liveRetrieval, liveSteps };
  });
};

interface ConversationActionsDependencies {
  queryClient: QueryClient;
  addRequestedId: (
    field: 'requestedLinkedConversationIds' | 'requestedWebSourceConversationIds' | 'requestedMembershipDocumentIds',
    id: string
  ) => void;
  lifecycle: ConversationLifecycleRegistry;
}

export function useConversationActions({ queryClient, addRequestedId, lifecycle }: ConversationActionsDependencies) {
  const invalidateLists = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
  }, [queryClient]);

  const loadSpaces = useCallback(async () => {
    try {
      await queryClient.fetchQuery({ ...spacesQueryOptions(), staleTime: 0 });
    } catch (error) {
      setUiError(error);
    }
  }, [queryClient]);

  // Switching to another space leaves the open chat behind: it belongs to the
  // space it was filed in, so keeping it on screen would show a conversation
  // the sidebar no longer lists. "All spaces" lists every chat and keeps it.
  const activeChatAfterSpaceChange = useCallback((spaceId: string | null): string | null => {
    const activeId = conversationUiStore.getState().activeConversationId;
    if (!activeId || !spaceId) return activeId;
    const detail = queryClient.getQueryData<Conversation | null>(conversationKeys.detail(activeId));
    return (detail?.spaceId ?? GENERAL_SPACE_ID) === spaceId ? activeId : null;
  }, [queryClient]);

  const setSelectedSpace = useCallback((spaceId: string | null) => {
    conversationUiStore.setState({
      selectedSpaceId: spaceId,
      activeConversationId: activeChatAfterSpaceChange(spaceId),
    });
  }, [activeChatAfterSpaceChange]);
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
      ...(params.spaceId !== current.selectedSpaceId
        ? { activeConversationId: activeChatAfterSpaceChange(params.spaceId) }
        : {}),
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
  }, [queryClient, activeChatAfterSpaceChange]);

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

  const createConversation = useCallback(async (title: string, spaceId?: string | null): Promise<string> => {
    // The sidebar's selection is where a chat the user starts belongs. A chat
    // made on behalf of another one names its space instead: what is selected
    // in the sidebar says nothing about the conversation being branched.
    const state = { selectedSpaceId: spaceId ?? conversationUiStore.getState().selectedSpaceId };
    const spaces = await queryClient.ensureQueryData(spacesQueryOptions());
    const selectedSpace = spaces.find(space => space.id === state.selectedSpaceId);
    if (state.selectedSpaceId && !selectedSpace) {
      const error = new Error('The selected space is unavailable. Select a space and try again.');
      setUiError(error);
      throw error;
    }
    const [activeModelsResult, settings] = await Promise.all([
      VaultAPI.getActiveModels(),
      queryClient.ensureQueryData(settingsQueryOptions()).catch(() => null),
    ]);
    const activeModel = activeModelsResult.ok ? activeModelsResult.data.chat_model : null;
    const llmSettings = settings?.llm ?? null;
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
    if (state.selectedSpaceId && state.selectedSpaceId !== GENERAL_SPACE_ID && selectedSpace) {
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
    try {
      await saveBookmark(queryClient, { conversationId, messageId, title, note });
      return true;
    } catch (error) {
      setUiError(error);
      return false;
    }
  }, [queryClient]);

  const unbookmarkMessage = useCallback(async (conversationId: string, messageId: string) => {
    try {
      await removeBookmark(queryClient, { conversationId, messageId });
      return true;
    } catch (error) {
      setUiError(error);
      return false;
    }
  }, [queryClient]);

  const moveConversationToSpace = useCallback(async (id: string, spaceId: string) => {
    const result = await VaultAPI.moveConversationToSpace({ conversationId: id, spaceId });
    if (!result.ok) {
      setUiError(result.error);
      return;
    }
    queryClient.setQueryData<Conversation | null>(conversationKeys.detail(id), current =>
      current ? { ...current, spaceId } : current);
    await Promise.all([invalidateLists(), queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks })]);
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
  }, [addRequestedId, queryClient]);

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
  }, [addRequestedId, queryClient]);

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
  }, [addRequestedId, queryClient]);

  const setDocumentSpaceMembership = useCallback(async (
    documentId: string,
    spaceId: string,
    assigned: boolean
  ) => {
    const result = await VaultAPI.setDocumentSpaceMembership(documentId, spaceId, assigned);
    if (!result.ok) setUiError(result.error);
    else await loadDocumentSpaceMemberships(documentId);
  }, [loadDocumentSpaceMemberships]);

  const selectConversation = useCallback(async function selectConversation(id: string): Promise<void> {
    conversationUiStore.setState({ activeConversationId: id, error: null });
    addRequestedId('requestedLinkedConversationIds', id);
    addRequestedId('requestedWebSourceConversationIds', id);
    try {
      const [, detail] = await Promise.all([
        queryClient.fetchQuery({ queryKey: conversationKeys.messages(id), queryFn: () => fetchMessages(id), staleTime: 0 }),
        queryClient.fetchQuery({ queryKey: conversationKeys.detail(id), queryFn: () => fetchConversationDetail(id), staleTime: 0 }),
        queryClient.fetchQuery({ queryKey: conversationKeys.bookmarks(id), queryFn: () => fetchBookmarks(id), staleTime: 0 }),
        queryClient.fetchQuery({ queryKey: conversationKeys.linkedDocuments(id), queryFn: () => fetchLinkedDocuments(id), staleTime: 0 }),
        queryClient.fetchQuery({ queryKey: conversationKeys.webSources(id), queryFn: () => fetchWebSources(id), staleTime: 0 }),
      ]);
      // The chat decides the space, not the other way round: a conversation
      // opened from "All spaces", the spotlight or a link searches its own
      // space's library, and a sidebar still showing another space would say
      // otherwise. Skipped if the reader has already moved on to another chat.
      const current = conversationUiStore.getState();
      if (detail?.tangentParentId && current.activeConversationId === id) {
        // References and deep links open a tangent beside its parent, never
        // as an extra entry in the main conversation list.
        conversationUiStore.setState({ requestedTangent: { parentId: detail.tangentParentId, tangentId: id } });
        await selectConversation(detail.tangentParentId);
        return;
      }
      const spaceId = detail?.spaceId;
      if (spaceId && current.activeConversationId === id && current.selectedSpaceId !== spaceId) {
        await loadConversations({ spaceId });
      }
    } catch (error) {
      // A slower read for the previous selection must not replace the error
      // state of the conversation the reader has since opened.
      if (conversationUiStore.getState().activeConversationId === id) setUiError(error);
    }
  }, [addRequestedId, queryClient, loadConversations]);

  /** Truncate persisted history after a message and refresh the conversation list. */
  const truncateAfter = useCallback(async (
    conversationId: string,
    messageId: string,
    inclusive = false
  ): Promise<boolean> => {
    let cancelled = false;
    const unregisterLifecycle = lifecycle.register(conversationId, () => { cancelled = true; });
    try {
      const result = await VaultAPI.truncateConversationAfter(conversationId, messageId, inclusive);
      // A truncate request can outlive a concurrent conversation deletion.
      // Never recreate its just-removed messages cache with a late response.
      if (cancelled) return false;
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
    } finally {
      unregisterLifecycle();
    }
  }, [invalidateLists, lifecycle, queryClient]);

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

  const continueInNewConversation = useCallback(async (conversationId: string): Promise<string> => {
    const result = await VaultAPI.continueInNewConversation(conversationId);
    if (!result.ok) throw new Error(result.error);
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
   * record is returned so the caller can render a divider. A refusal ("nothing
   * to compact", no model) is returned rather than raised: the chat says it in
   * place, where `/compact` was typed.
   */
  const compactConversation = useCallback(async (
    conversationId: string,
    keepRecentMessages?: number
  ): Promise<CompactionResult> => {
    const result = await VaultAPI.compactConversation(conversationId, keepRecentMessages);
    if (!result.ok) return { ok: false, error: result.error };
    await Promise.all([
      invalidateLists(),
      // The detail read is what carries the compaction back after a reload.
      queryClient.invalidateQueries({
        queryKey: conversationKeys.detail(conversationId),
      }),
    ]);
    return { ok: true, record: result.data.compaction };
  }, [invalidateLists, queryClient]);

  const deleteMessage = useCallback(async (conversationId: string, messageId: string) => {
    const key = conversationKeys.messages(conversationId);
    const previous = queryClient.getQueryData<ConversationMessage[]>(key);
    if (!previous?.some(message => message.id === messageId)) {
      setUiError('Message not found in the selected conversation.');
      return;
    }
    let cancelled = false;
    const unregisterLifecycle = lifecycle.register(conversationId, () => { cancelled = true; });
    queryClient.setQueryData(key, previous.filter(message => message.id !== messageId));
    try {
      const result = await VaultAPI.deleteConversationMessage({ conversationId, messageId });
      if (cancelled) return;
      if (!result.ok) {
        queryClient.setQueryData<ConversationMessage[]>(key, current =>
          restoreMissingItem(current, previous, messageId));
        setUiError(result.error);
        return;
      }
      await Promise.all([
        invalidateLists(),
        queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks }),
      ]);
    } finally {
      unregisterLifecycle();
    }
  }, [invalidateLists, lifecycle, queryClient]);

  const deleteConversation = useCallback(async (id: string) => {
    // No list pre-check: the open conversation can be one the current filter
    // or search hides, and the backend is the authority on whether it exists.
    // Stop a request started before the delete from writing its stale result
    // over the optimistic removal. A successful delete then refetches every
    // active list using the backend's post-delete state.
    await queryClient.cancelQueries({ queryKey: conversationKeys.lists });
    const snapshots = queryClient.getQueriesData<Conversation[]>({ queryKey: conversationKeys.lists });
    queryClient.setQueriesData<Conversation[]>({ queryKey: conversationKeys.lists }, current =>
      current?.filter(item => item.id !== id)
    );
    const wasActive = conversationUiStore.getState().activeConversationId === id;
    if (wasActive) conversationUiStore.setState({ activeConversationId: null });
    const result = await VaultAPI.deleteConversation(id);
    if (!result.ok) {
      for (const [key, snapshot] of snapshots) {
        if (!snapshot) continue;
        queryClient.setQueryData<Conversation[]>(key, current => restoreMissingItem(current, snapshot, id));
      }
      if (wasActive && conversationUiStore.getState().activeConversationId === null) {
        conversationUiStore.setState({ activeConversationId: id });
      }
      setUiError(result.error);
      return false;
    }
    const children = queryClient.getQueryData<ConversationTangentDto[]>(conversationKeys.tangents(id)) ?? [];
    for (const removedId of [id, ...children.map(child => child.conversationId)]) {
      lifecycle.cleanupConversation(removedId);
      forgetRequestedConversation(removedId);
      queryClient.removeQueries({ queryKey: conversationKeys.detail(removedId), exact: true });
      queryClient.removeQueries({ queryKey: conversationKeys.messages(removedId), exact: true });
      queryClient.removeQueries({ queryKey: conversationKeys.bookmarks(removedId) });
      queryClient.removeQueries({ queryKey: conversationKeys.linkedDocuments(removedId), exact: true });
      queryClient.removeQueries({ queryKey: conversationKeys.webSources(removedId), exact: true });
      queryClient.removeQueries({ queryKey: conversationKeys.tangents(removedId), exact: true });
    }
    await Promise.all([invalidateLists(), queryClient.invalidateQueries({ queryKey: conversationKeys.allBookmarks })]);
    return true;
  }, [invalidateLists, lifecycle, queryClient]);

  const dismissFailedMessage = useCallback((tempId: string) => {
    conversationUiStore.setState(current => {
      if (current.optimisticMessages.get(tempId)?.status !== 'failed') return current;
      const optimisticMessages = new Map(current.optimisticMessages);
      optimisticMessages.delete(tempId);
      return { optimisticMessages };
    });
  }, []);

  /** Pure UI state: the text the composer should adopt on its next render. */
  const setComposerDraft = useCallback(
    (draft: string | null) => conversationUiStore.setState({ composerDraft: draft }),
    []
  );

  const clearError = useCallback(() => conversationUiStore.setState({ error: null }), []);

  return {
    invalidateLists, loadSpaces, setSelectedSpace, setFilterMode, setSearchQuery,
    loadConversations, loadMessageBookmarks, createConversation,
    setConversationSaved, setConversationBookmarked, setConversationPinned,
    setConversationArchived, renameConversation, bookmarkMessage, unbookmarkMessage,
    moveConversationToSpace, loadConversationLinkedDocuments, loadConversationWebSources,
    addConversationWebSource, removeConversationWebSource, removeConversationLinkedDocument,
    loadDocumentSpaceMemberships, setDocumentSpaceMembership, selectConversation,
    truncateAfter, forkConversation, continueInNewConversation, compactConversation,
    deleteMessage, deleteConversation, dismissFailedMessage, setComposerDraft, clearError,
  };
}
