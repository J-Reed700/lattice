import { useCallback } from 'react';

import { useApi } from '@/contexts/ApiContext';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type {
  ConversationLinkedDocumentDto, ConversationWebSourceDto,
} from '@/types';

import { setUiError, type ConversationActionsDependencies } from './conversationActionHelpers';
import { fetchLinkedDocuments, fetchMemberships, fetchWebSources } from '../api/conversationQueryData';

/** What a conversation draws on beyond its space: linked documents, web sources and space memberships. */
export function useConversationSourceActions({
  queryClient,
  addRequestedId,
}: Pick<ConversationActionsDependencies, 'queryClient' | 'addRequestedId'>) {
  const api = useApi();

  const loadConversationLinkedDocuments = useCallback(async (conversationId: string) => {
    addRequestedId('requestedLinkedConversationIds', conversationId);
    try {
      await queryClient.fetchQuery({
        queryKey: conversationKeys.linkedDocuments(conversationId),
        queryFn: () => fetchLinkedDocuments(conversationId, api),
        staleTime: 0,
      });
    } catch (error) {
      setUiError(error);
    }
  }, [addRequestedId, api, queryClient]);

  const loadConversationWebSources = useCallback(async (conversationId: string) => {
    addRequestedId('requestedWebSourceConversationIds', conversationId);
    try {
      await queryClient.fetchQuery({
        queryKey: conversationKeys.webSources(conversationId),
        queryFn: () => fetchWebSources(conversationId, api),
        staleTime: 0,
      });
    } catch (error) {
      setUiError(error);
    }
  }, [addRequestedId, api, queryClient]);

  const addConversationWebSource = useCallback(async (
    conversationId: string,
    url: string,
    options?: { title?: string; excerpt?: string; relevanceScore?: number }
  ): Promise<boolean> => {
    const result = await api.addConversationWebSource(conversationId, url, options);
    if (!result.ok) {
      setUiError(result.error);
      return false;
    }
    await loadConversationWebSources(conversationId);
    return true;
  }, [api, loadConversationWebSources]);

  const removeConversationWebSource = useCallback(async (
    conversationId: string,
    sourceId: string
  ): Promise<boolean> => {
    const result = await api.removeConversationWebSource(conversationId, sourceId);
    if (!result.ok) {
      setUiError(result.error);
      return false;
    }
    queryClient.setQueryData<ConversationWebSourceDto[]>(
      conversationKeys.webSources(conversationId),
      current => current?.filter(source => source.id !== sourceId)
    );
    return true;
  }, [api, queryClient]);

  const removeConversationLinkedDocument = useCallback(async (
    conversationId: string,
    documentId: string
  ) => {
    const result = await api.removeConversationLinkedDocument(conversationId, documentId);
    if (!result.ok) {
      setUiError(result.error);
      return;
    }
    queryClient.setQueryData<ConversationLinkedDocumentDto[]>(
      conversationKeys.linkedDocuments(conversationId),
      current => current?.filter(document => document.documentId !== documentId)
    );
    queryClient.removeQueries({ queryKey: conversationKeys.memberships(documentId), exact: true });
  }, [api, queryClient]);

  const loadDocumentSpaceMemberships = useCallback(async (documentId: string) => {
    addRequestedId('requestedMembershipDocumentIds', documentId);
    try {
      await queryClient.fetchQuery({
        queryKey: conversationKeys.memberships(documentId),
        queryFn: () => fetchMemberships(documentId, api),
        staleTime: 0,
      });
    } catch (error) {
      setUiError(error);
    }
  }, [addRequestedId, api, queryClient]);

  const setDocumentSpaceMembership = useCallback(async (
    documentId: string,
    spaceId: string,
    assigned: boolean
  ) => {
    const result = await api.setDocumentSpaceMembership(documentId, spaceId, assigned);
    if (!result.ok) setUiError(result.error);
    else await loadDocumentSpaceMemberships(documentId);
  }, [api, loadDocumentSpaceMemberships]);

  return {
    loadConversationLinkedDocuments, loadConversationWebSources,
    addConversationWebSource, removeConversationWebSource, removeConversationLinkedDocument,
    loadDocumentSpaceMemberships, setDocumentSpaceMembership,
  };
}
