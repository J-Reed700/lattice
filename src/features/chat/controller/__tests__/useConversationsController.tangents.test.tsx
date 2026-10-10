import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useConversationsController } from '@/features/chat/controller/useConversationsController';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { VaultAPI } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;
const parent = { id: 'parent', title: 'Main conversation', updatedAt: '2026-10-06T00:00:00Z' };
const message = { id: 'question', conversationId: 'tangent', role: 'user', content: 'Tangent question', status: 'completed', tokens: 2, createdAt: '2026-10-06T00:00:00Z' };

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return { client, ...renderHook(useConversationsController, { wrapper }) };
}

describe('shared conversation lifecycle with tangents', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    conversationUiStore.setState({ activeConversationId: 'parent', selectedSpaceId: null, filterMode: 'all', searchQuery: '',
      requestedTangent: null,
      composerDraft: 'Main draft', optimisticMessages: new Map(), inFlightGenerations: new Map(), liveRetrieval: new Map(), liveSteps: new Map(), error: null,
      requestedLinkedConversationIds: new Set(), requestedWebSourceConversationIds: new Set(), requestedMembershipDocumentIds: new Set() });
    api.listConversationSpaces = vi.fn().mockResolvedValue({ ok: true, data: [] });
    api.listConversationsExplorer = vi.fn().mockResolvedValue({ ok: true, data: { conversations: [parent], total: 1 } });
    api.getConversation = vi.fn().mockResolvedValue({ ok: true, data: { conversation: parent } });
    api.getConversationMessages = vi.fn().mockResolvedValue({ ok: true, data: { messages: [], total: 0 } });
    api.listMessageBookmarks = vi.fn().mockResolvedValue({ ok: true, data: { bookmarks: [], total: 0 } });
    api.listConversationLinkedDocuments = vi.fn().mockResolvedValue({ ok: true, data: [] });
    api.listConversationWebSources = vi.fn().mockResolvedValue({ ok: true, data: [] });
  });

  it('streams into the tangent cache without ever inserting it into a main list or selecting it', async () => {
    api.chatWithConversation = vi.fn().mockResolvedValue({ ok: true, data: { conversationId: 'tangent', messages: [message], contextUsed: 1 } });
    const { result, client } = setup();
    await waitFor(() => expect(result.current.conversations).toHaveLength(1));
    const key = conversationKeys.list({ spaceId: null, filterMode: 'all', searchQuery: '' });
    const seen: string[][] = [];
    const unsubscribe = client.getQueryCache().subscribe(() => {
      seen.push((client.getQueryData<{ id: string }[]>(key) ?? []).map(item => item.id));
    });
    await act(async () => { expect(await result.current.sendMessage('Tangent question', 'tangent')).toBe('answered'); });
    unsubscribe();
    expect(seen.every(ids => !ids.includes('tangent'))).toBe(true);
    expect(client.getQueryData(conversationKeys.messages('tangent'))).toEqual([message]);
    expect(client.getQueryData(conversationKeys.messages('parent'))).toEqual([]);
    expect(conversationUiStore.getState().activeConversationId).toBe('parent');
    expect(conversationUiStore.getState().composerDraft).toBe('Main draft');
  });

  it('does not restore a failed tangent regenerate into the parent composer', async () => {
    api.regenerateResponse.mockResolvedValue({ ok: false, error: 'No model available' });
    const { result, client } = setup();
    client.setQueryData(conversationKeys.messages('tangent'), [message]);
    await act(async () => { expect(await result.current.regenerateResponse('tangent')).toBe('failed'); });
    expect(conversationUiStore.getState().composerDraft).toBe('Main draft');
    expect(conversationUiStore.getState().activeConversationId).toBe('parent');
  });

  it('opens a linked tangent under its parent without adding it to the conversation list', async () => {
    api.getConversation.mockImplementation(async (id: string) => ({ ok: true, data: { conversation: id === 'tangent'
      ? { ...parent, id: 'tangent', tangentParentId: 'parent' } : parent } }));
    const { result } = setup();
    await act(async () => { await result.current.selectConversation('tangent'); });
    expect(conversationUiStore.getState().activeConversationId).toBe('parent');
    expect(conversationUiStore.getState().requestedTangent).toEqual({ parentId: 'parent', tangentId: 'tangent' });
    expect(result.current.conversations.every(item => item.id !== 'tangent')).toBe(true);
    expect(conversationUiStore.getState().composerDraft).toBe('Main draft');
  });
});
