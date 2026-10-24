import { QueryClient } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { spaceKeys } from '@/features/spaces/api/queries';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { deferred } from '@/tests/deferred';

import { useConversationNavigationActions } from '../useConversationNavigationActions';

const h = vi.hoisted(() => ({
  api: {
    listConversationSpaces: vi.fn(),
    listConversationsExplorer: vi.fn(),
    listConversations: vi.fn(),
    listMessageBookmarks: vi.fn(),
    getConversationMessages: vi.fn(),
    getConversation: vi.fn(),
    listConversationLinkedDocuments: vi.fn(),
    listConversationWebSources: vi.fn(),
  },
}));
vi.mock('@/lib/api', () => ({ VaultAPI: h.api, default: h.api }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const refused = (error: string) => ({ ok: false as const, error });

let queryClient: QueryClient;
const setup = () => renderHook(() => useConversationNavigationActions({ queryClient, addRequestedId: vi.fn() })).result.current;
const ui = () => conversationUiStore.getState();

describe('useConversationNavigationActions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    conversationUiStore.setState({ selectedSpaceId: null, activeConversationId: null, filterMode: 'all', searchQuery: '', error: null, requestedTangent: null } as never);
    h.api.listConversationsExplorer.mockResolvedValue(ok({ conversations: [] }));
    h.api.listMessageBookmarks.mockResolvedValue(ok({ bookmarks: [] }));
    h.api.getConversationMessages.mockResolvedValue(ok({ messages: [] }));
    h.api.listConversationLinkedDocuments.mockResolvedValue(ok([]));
    h.api.listConversationWebSources.mockResolvedValue(ok([]));
  });

  it('loads spaces, showing a failure', async () => {
    h.api.listConversationSpaces.mockResolvedValueOnce(ok([{ id: 's-hist', name: 'History' }])).mockResolvedValueOnce(refused('spaces unreadable'));
    const actions = setup();
    await act(() => actions.loadSpaces());
    expect(queryClient.getQueryData(spaceKeys.list)).toEqual([{ id: 's-hist', name: 'History' }]);
    await act(() => actions.loadSpaces());
    expect(ui().error).toBe('spaces unreadable');
  });

  it('sets the filter and the search query', () => {
    const actions = setup();
    act(() => actions.setFilterMode('pinned'));
    act(() => actions.setSearchQuery('carthage'));
    expect(ui()).toMatchObject({ filterMode: 'pinned', searchQuery: 'carthage' });
  });

  describe('switching space', () => {
    it('keeps the open chat when it belongs to the new space, or for all spaces', () => {
      queryClient.setQueryData(conversationKeys.detail('c1'), { id: 'c1', spaceId: 's-hist' });
      conversationUiStore.setState({ activeConversationId: 'c1' } as never);
      const actions = setup();
      act(() => actions.setSelectedSpace('s-hist'));
      expect(ui().activeConversationId).toBe('c1');
      act(() => actions.setSelectedSpace(null));
      expect(ui().activeConversationId).toBe('c1');
    });

    it('closes the open chat when it belongs elsewhere, counting no space as General', () => {
      queryClient.setQueryData(conversationKeys.detail('c1'), { id: 'c1', spaceId: null });
      conversationUiStore.setState({ activeConversationId: 'c1' } as never);
      const actions = setup();
      act(() => actions.setSelectedSpace(GENERAL_SPACE_ID));
      expect(ui().activeConversationId).toBe('c1');
      act(() => actions.setSelectedSpace('s-hist'));
      expect(ui().activeConversationId).toBeNull();
    });
  });

  describe('loadConversations', () => {
    it('lists with the current filters, overriding what is given', async () => {
      conversationUiStore.setState({ filterMode: 'saved', searchQuery: 'rome' } as never);
      h.api.listConversationsExplorer.mockResolvedValueOnce(ok({ conversations: [{ id: 'c1' }] }));
      const actions = setup();
      await act(() => actions.loadConversations({ spaceId: 's-hist', searchQuery: 'carthage' }));
      expect(ui()).toMatchObject({ selectedSpaceId: 's-hist', filterMode: 'saved', searchQuery: 'carthage' });
      expect(h.api.listConversationsExplorer).toHaveBeenCalled();
    });

    it('shows a list that could not load', async () => {
      h.api.listConversationsExplorer.mockResolvedValueOnce(refused('explorer gone'));
      h.api.listConversations.mockResolvedValueOnce(refused('db locked'));
      const actions = setup();
      await act(() => actions.loadConversations());
      expect(ui().error).toBe('db locked');
    });
  });

  describe('loadMessageBookmarks', () => {
    it('loads for the open chat with a trimmed query, or does nothing without one', async () => {
      const actions = setup();
      await act(() => actions.loadMessageBookmarks());
      expect(h.api.listMessageBookmarks).not.toHaveBeenCalled();
      conversationUiStore.setState({ activeConversationId: 'c1' } as never);
      await act(() => actions.loadMessageBookmarks({ query: '  nile  ' }));
      expect(h.api.listMessageBookmarks).toHaveBeenCalledWith(expect.objectContaining({ conversationId: 'c1', query: 'nile' }));
      h.api.listMessageBookmarks.mockResolvedValueOnce(refused('bookmarks locked'));
      await act(() => actions.loadMessageBookmarks({ conversationId: 'c2' }));
      expect(ui().error).toBe('bookmarks locked');
    });
  });

  describe('selectConversation', () => {
    it('opens a chat and moves the sidebar to its space', async () => {
      h.api.getConversation.mockResolvedValue(ok({ conversation: { id: 'c1', spaceId: 's-hist' } }));
      const actions = setup();
      await act(() => actions.selectConversation('c1'));
      expect(ui()).toMatchObject({ activeConversationId: 'c1', selectedSpaceId: 's-hist' });
    });

    it('opens a tangent beside its parent', async () => {
      h.api.getConversation.mockImplementation(async (id: string) => ok({ conversation: id === 't1' ? { id: 't1', tangentParentId: 'p1' } : { id, spaceId: null } }));
      const actions = setup();
      await act(() => actions.selectConversation('t1'));
      expect(ui().activeConversationId).toBe('p1');
      expect(ui().requestedTangent).toEqual({ parentId: 'p1', tangentId: 't1' });
    });

    it('shows a failure only for the chat still open', async () => {
      const slow = deferred<unknown>();
      h.api.getConversationMessages.mockReturnValueOnce(slow.promise);
      h.api.getConversation.mockResolvedValue(ok({ conversation: { id: 'x', spaceId: null } }));
      const actions = setup();
      let first!: Promise<void>;
      act(() => { first = actions.selectConversation('c1'); });
      await act(() => actions.selectConversation('c2'));
      await act(async () => { slow.resolve(refused('c1 unreadable')); await first; });
      expect(ui().error).toBeNull();
      h.api.getConversationMessages.mockResolvedValueOnce(refused('c3 unreadable'));
      await act(() => actions.selectConversation('c3'));
      expect(ui().error).toBe('c3 unreadable');
    });
  });
});
