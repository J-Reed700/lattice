import { useCallback } from 'react';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { spacesQueryOptions } from '@/features/spaces/api/queries';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { conversationKeys, type ConversationListParams } from '@/shared/conversations/conversationKeys';
import type { ConversationsState, LoadConversationsOverrides, LoadMessageBookmarksOverrides } from '@/shared/conversations/conversationsStore.types';
import type { Conversation } from '@/types/conversation';

import { setUiError, type ConversationActionsDependencies } from './conversationActionHelpers';
import { fetchBookmarks, fetchConversationDetail, fetchConversationList, fetchLinkedDocuments, fetchMessages, fetchWebSources } from '../api/conversationQueryData';

/** Which space and chats the sidebar lists, and opening a conversation. */
export function useConversationNavigationActions({
  queryClient,
  addRequestedId,
}: Pick<ConversationActionsDependencies, 'queryClient' | 'addRequestedId'>) {

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

  return {
    invalidateLists, loadSpaces, setSelectedSpace, setFilterMode, setSearchQuery,
    loadConversations, loadMessageBookmarks, selectConversation,
  };
}
