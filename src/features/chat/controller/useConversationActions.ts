import { useCallback } from 'react';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { settingsQueryOptions } from '@/features/settings/hooks/useSettingsQuery';
import { spacesQueryOptions } from '@/features/spaces/api/queries';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { VaultAPI } from '@/lib/api';
import type { ConversationTangentDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { CompactionResult } from '@/shared/conversations/conversationsStore.types';
import type { Conversation } from '@/types/conversation';
import { resolveChatModel } from '@/utils/chatModelSelection';

import {
  forgetRequestedConversation,
  restoreMissingItem,
  setUiError,
  type ConversationActionsDependencies,
} from './conversationActionHelpers';
import { useConversationMessageActions } from './useConversationMessageActions';
import { useConversationNavigationActions } from './useConversationNavigationActions';
import { useConversationSourceActions } from './useConversationSourceActions';
import { fetchConversationDetail } from '../api/conversationQueryData';

export const MAX_OBSERVED_CONVERSATIONS = 20;
export const MAX_OBSERVED_MEMBERSHIPS = 100;

export function useConversationActions({ queryClient, addRequestedId, lifecycle }: ConversationActionsDependencies) {
  const {
    invalidateLists, loadSpaces, setSelectedSpace, setFilterMode, setSearchQuery,
    loadConversations, loadMessageBookmarks, selectConversation,
  } = useConversationNavigationActions({ queryClient, addRequestedId });
  const {
    loadConversationLinkedDocuments, loadConversationWebSources,
    addConversationWebSource, removeConversationWebSource, removeConversationLinkedDocument,
    loadDocumentSpaceMemberships, setDocumentSpaceMembership,
  } = useConversationSourceActions({ queryClient, addRequestedId });
  const {
    bookmarkMessage, unbookmarkMessage, truncateAfter, deleteMessage,
    dismissFailedMessage, setComposerDraft, clearError,
  } = useConversationMessageActions({ queryClient, lifecycle, invalidateLists });


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
