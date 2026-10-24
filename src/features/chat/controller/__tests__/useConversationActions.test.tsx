import { QueryClient } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { spaceKeys } from '@/features/spaces/api/queries';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { conversationDraftKey, setDraft, useDraftStore } from '@/shared/drafts/drafts';

import { useConversationActions } from '../useConversationActions';

const h = vi.hoisted(() => ({
  api: {
    createConversation: vi.fn(),
    moveConversationToSpace: vi.fn(),
    getConversation: vi.fn(),
    getConversationMessages: vi.fn(),
    listMessageBookmarks: vi.fn(),
    listConversationLinkedDocuments: vi.fn(),
    listConversationWebSources: vi.fn(),
    setConversationModelChoice: vi.fn(),
    setConversationSaved: vi.fn(),
    setConversationBookmarked: vi.fn(),
    setConversationPinned: vi.fn(),
    setConversationArchived: vi.fn(),
    renameConversation: vi.fn(),
    forkConversation: vi.fn(),
    continueInNewConversation: vi.fn(),
    compactConversation: vi.fn(),
    deleteConversation: vi.fn(),
  },
}));
vi.mock('@/lib/api', () => ({ VaultAPI: h.api, default: h.api }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const refused = (error: string) => ({ ok: false as const, error });

let queryClient: QueryClient;
const lifecycle = { cleanupConversation: vi.fn() };

const setup = () => {
  const { result } = renderHook(() => useConversationActions({ queryClient, addRequestedId: vi.fn(), lifecycle: lifecycle as never }));
  return result.current;
};

const listKey = [...conversationKeys.lists, { spaceId: null }];
const uiError = () => conversationUiStore.getState().error;

describe('useConversationActions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(spaceKeys.list, [
      { id: GENERAL_SPACE_ID, name: 'General' },
      { id: 's-hist', name: 'History' },
    ]);
    conversationUiStore.setState({ selectedSpaceId: null, activeConversationId: null, error: null } as never);
    h.api.getConversationMessages.mockResolvedValue(ok({ messages: [] }));
    h.api.getConversation.mockResolvedValue(ok({ conversation: { id: 'x', spaceId: null } }));
    h.api.listMessageBookmarks.mockResolvedValue(ok({ bookmarks: [] }));
    h.api.listConversationLinkedDocuments.mockResolvedValue(ok([]));
    h.api.listConversationWebSources.mockResolvedValue(ok([]));
  });

  describe('createConversation', () => {
    it('creates a chat in General and opens it', async () => {
      h.api.createConversation.mockResolvedValueOnce(ok({ conversation: { id: 'c1', title: 'New chat', spaceId: null } }));
      const actions = setup();
      let id!: string;
      await act(async () => { id = await actions.createConversation('New chat'); });
      expect(id).toBe('c1');
      expect(h.api.moveConversationToSpace).not.toHaveBeenCalled();
      expect(conversationUiStore.getState().activeConversationId).toBe('c1');
      expect(queryClient.getQueryData(conversationKeys.messages('c1'))).toEqual([]);
    });

    it('files the new chat in the selected space and reads it back', async () => {
      conversationUiStore.setState({ selectedSpaceId: 's-hist' } as never);
      h.api.createConversation.mockResolvedValueOnce(ok({ conversation: { id: 'c1', spaceId: null } }));
      h.api.moveConversationToSpace.mockResolvedValueOnce(ok(null));
      h.api.getConversation.mockResolvedValueOnce(ok({ conversation: { id: 'c1', spaceId: 's-hist' } }));
      const actions = setup();
      await act(async () => { await actions.createConversation('New chat'); });
      expect(h.api.moveConversationToSpace).toHaveBeenCalledWith({ conversationId: 'c1', spaceId: 's-hist' });
      expect(queryClient.getQueryData(conversationKeys.detail('c1'))).toEqual({ id: 'c1', spaceId: 's-hist' });
    });

    it('uses the space it is given over the sidebar’s selection', async () => {
      conversationUiStore.setState({ selectedSpaceId: 's-hist' } as never);
      h.api.createConversation.mockResolvedValueOnce(ok({ conversation: { id: 'c1', spaceId: null } }));
      const actions = setup();
      await act(async () => { await actions.createConversation('Branch', GENERAL_SPACE_ID); });
      expect(h.api.moveConversationToSpace).not.toHaveBeenCalled();
    });

    it.each([
      ['an unavailable space', () => { conversationUiStore.setState({ selectedSpaceId: 's-gone' } as never); }, 'The selected space is unavailable. Select a space and try again.'],
      ['a refused create', () => { h.api.createConversation.mockResolvedValueOnce(refused('db locked')); }, 'db locked'],
      ['a refused move', () => {
        conversationUiStore.setState({ selectedSpaceId: 's-hist' } as never);
        h.api.createConversation.mockResolvedValueOnce(ok({ conversation: { id: 'c1' } }));
        h.api.moveConversationToSpace.mockResolvedValueOnce(refused('space archived'));
      }, 'Could not create the chat in History: space archived'],
      ['a move that did not stick', () => {
        conversationUiStore.setState({ selectedSpaceId: 's-hist' } as never);
        h.api.createConversation.mockResolvedValueOnce(ok({ conversation: { id: 'c1' } }));
        h.api.moveConversationToSpace.mockResolvedValueOnce(ok(null));
        h.api.getConversation.mockResolvedValueOnce(ok({ conversation: { id: 'c1', spaceId: null } }));
      }, 'Could not confirm the new chat’s space. Refresh and try again.'],
    ])('throws and shows %s', async (_label, arrange, message) => {
      arrange();
      const actions = setup();
      await act(async () => { await expect(actions.createConversation('New chat')).rejects.toThrow(message); });
      expect(uiError()).toBe(message);
    });
  });

  describe('setConversationModelChoice', () => {
    it('saves the chat’s model and updates the caches', async () => {
      queryClient.setQueryData(conversationKeys.detail('c1'), { id: 'c1', title: 'Rome' });
      queryClient.setQueryData(listKey, [{ id: 'c1' }, { id: 'c2' }]);
      h.api.setConversationModelChoice.mockResolvedValueOnce(ok({ id: 'c1', modelChoice: { kind: 'model', id: 'gemma' } }));
      const actions = setup();
      await act(async () => { await actions.setConversationModelChoice('c1', { kind: 'model' } as never); });
      expect(queryClient.getQueryData(conversationKeys.detail('c1'))).toMatchObject({ title: 'Rome', modelChoice: { kind: 'model', id: 'gemma' } });
      expect(queryClient.getQueryData(listKey)).toEqual([{ id: 'c1', modelChoice: { kind: 'model', id: 'gemma' } }, { id: 'c2' }]);
    });

    it('seeds the detail when none is cached, and throws a refusal', async () => {
      h.api.setConversationModelChoice.mockResolvedValueOnce(ok({ id: 'c3', modelChoice: { kind: 'default' } }));
      const actions = setup();
      await act(async () => { await actions.setConversationModelChoice('c3', { kind: 'default' } as never); });
      expect(queryClient.getQueryData(conversationKeys.detail('c3'))).toEqual({ id: 'c3', modelChoice: { kind: 'default' } });
      h.api.setConversationModelChoice.mockResolvedValueOnce(refused('model missing'));
      await act(async () => { await expect(actions.setConversationModelChoice('c3', { kind: 'default' } as never)).rejects.toThrow('model missing'); });
    });
  });

  describe('flags', () => {
    it('sets saved, bookmarked and pinned', async () => {
      h.api.setConversationSaved.mockResolvedValueOnce(ok(null));
      h.api.setConversationBookmarked.mockResolvedValueOnce(ok(null));
      h.api.setConversationPinned.mockResolvedValueOnce(refused('locked'));
      const actions = setup();
      await act(() => actions.setConversationSaved('c1', true));
      await act(() => actions.setConversationBookmarked('c1', false));
      expect(h.api.setConversationSaved).toHaveBeenCalledWith({ conversationId: 'c1', value: true });
      expect(h.api.setConversationBookmarked).toHaveBeenCalledWith({ conversationId: 'c1', value: false });
      await act(() => actions.setConversationPinned('c1', true));
      expect(uiError()).toBe('locked');
    });

    it('closes a chat when it is archived', async () => {
      conversationUiStore.setState({ activeConversationId: 'c1' } as never);
      h.api.setConversationArchived.mockResolvedValue(ok(null));
      const actions = setup();
      await act(() => actions.setConversationArchived('c2', true));
      expect(conversationUiStore.getState().activeConversationId).toBe('c1');
      await act(() => actions.setConversationArchived('c1', false));
      expect(conversationUiStore.getState().activeConversationId).toBe('c1');
      await act(() => actions.setConversationArchived('c1', true));
      expect(conversationUiStore.getState().activeConversationId).toBeNull();
    });
  });

  describe('renameConversation', () => {
    it('refuses a blank title', async () => {
      const actions = setup();
      let renamed!: boolean;
      await act(async () => { renamed = await actions.renameConversation('c1', '   '); });
      expect(renamed).toBe(false);
      expect(uiError()).toBe('Conversation title cannot be empty.');
      expect(h.api.renameConversation).not.toHaveBeenCalled();
    });

    it('renames in the lists and the detail', async () => {
      queryClient.setQueryData(conversationKeys.detail('c1'), { id: 'c1', title: 'Old' });
      queryClient.setQueryData(listKey, [{ id: 'c1', title: 'Old' }, { id: 'c2', title: 'Other' }]);
      h.api.renameConversation.mockResolvedValueOnce(ok(null));
      const actions = setup();
      let renamed!: boolean;
      await act(async () => { renamed = await actions.renameConversation('c1', '  Punic Wars  '); });
      expect(renamed).toBe(true);
      expect(h.api.renameConversation).toHaveBeenCalledWith('c1', 'Punic Wars');
      expect(queryClient.getQueryData(conversationKeys.detail('c1'))).toMatchObject({ title: 'Punic Wars' });
      expect((queryClient.getQueryData(listKey) as Array<{ title: string }>).map((item) => item.title)).toEqual(['Punic Wars', 'Other']);
    });

    it('leaves an uncached detail alone, and reports a refusal', async () => {
      h.api.renameConversation.mockResolvedValueOnce(ok(null)).mockResolvedValueOnce(refused('title too long'));
      const actions = setup();
      await act(async () => { await actions.renameConversation('c9', 'X'); });
      expect(queryClient.getQueryData(conversationKeys.detail('c9'))).toBeUndefined();
      let renamed!: boolean;
      await act(async () => { renamed = await actions.renameConversation('c9', 'Y'); });
      expect(renamed).toBe(false);
      expect(uiError()).toBe('title too long');
    });
  });

  describe('moveConversationToSpace', () => {
    it('moves the chat and updates its cached space', async () => {
      queryClient.setQueryData(conversationKeys.detail('c1'), { id: 'c1', spaceId: null });
      h.api.moveConversationToSpace.mockResolvedValueOnce(ok(null)).mockResolvedValueOnce(ok(null)).mockResolvedValueOnce(refused('space archived'));
      const actions = setup();
      await act(() => actions.moveConversationToSpace('c1', 's-hist'));
      expect(queryClient.getQueryData(conversationKeys.detail('c1'))).toEqual({ id: 'c1', spaceId: 's-hist' });
      await act(() => actions.moveConversationToSpace('c9', 's-hist'));
      expect(queryClient.getQueryData(conversationKeys.detail('c9'))).toBeUndefined();
      await act(() => actions.moveConversationToSpace('c1', 's-old'));
      expect(uiError()).toBe('space archived');
    });
  });

  describe('branching and continuing', () => {
    it('forks and opens the branch, or reports why not', async () => {
      h.api.forkConversation.mockResolvedValueOnce(ok({ conversation: { id: 'b1' } })).mockResolvedValueOnce(refused('message gone'));
      const actions = setup();
      let id!: string | null;
      await act(async () => { id = await actions.forkConversation('c1', 'm3'); });
      expect(id).toBe('b1');
      expect(h.api.forkConversation).toHaveBeenCalledWith('c1', 'm3');
      expect(conversationUiStore.getState().activeConversationId).toBe('b1');
      await act(async () => { id = await actions.forkConversation('c1'); });
      expect(id).toBeNull();
      expect(uiError()).toBe('message gone');
    });

    it('continues in a new chat, or throws why not', async () => {
      h.api.continueInNewConversation.mockResolvedValueOnce(ok({ conversation: { id: 'n1' } })).mockResolvedValueOnce(refused('no model'));
      const actions = setup();
      let id!: string;
      await act(async () => { id = await actions.continueInNewConversation('c1'); });
      expect(id).toBe('n1');
      expect(conversationUiStore.getState().activeConversationId).toBe('n1');
      await act(async () => { await expect(actions.continueInNewConversation('c1')).rejects.toThrow('no model'); });
    });
  });

  describe('compactConversation', () => {
    it('returns the record, or the refusal', async () => {
      h.api.compactConversation.mockResolvedValueOnce(ok({ compaction: { upToMessageId: 'm4' } })).mockResolvedValueOnce(refused('nothing to compact'));
      const actions = setup();
      let outcome!: unknown;
      await act(async () => { outcome = await actions.compactConversation('c1', 4); });
      expect(outcome).toEqual({ ok: true, record: { upToMessageId: 'm4' } });
      expect(h.api.compactConversation).toHaveBeenCalledWith('c1', 4);
      await act(async () => { outcome = await actions.compactConversation('c1'); });
      expect(outcome).toEqual({ ok: false, error: 'nothing to compact' });
    });
  });

  describe('deleteConversation', () => {
    it('removes the chat, its tangents and their drafts', async () => {
      queryClient.setQueryData(listKey, [{ id: 'c1' }, { id: 'c2' }]);
      queryClient.setQueryData(conversationKeys.tangents('c1'), [{ conversationId: 't1' }]);
      queryClient.setQueryData(conversationKeys.messages('t1'), []);
      setDraft(conversationDraftKey('c1'), 'half a question');
      conversationUiStore.setState({ activeConversationId: 'c1' } as never);
      h.api.deleteConversation.mockResolvedValueOnce(ok(null));
      const actions = setup();
      let deleted!: boolean;
      await act(async () => { deleted = await actions.deleteConversation('c1'); });
      expect(deleted).toBe(true);
      expect(conversationUiStore.getState().activeConversationId).toBeNull();
      expect(lifecycle.cleanupConversation.mock.calls).toEqual([['c1'], ['t1']]);
      expect(useDraftStore.getState().drafts[conversationDraftKey('c1')]).toBeUndefined();
      expect(queryClient.getQueryData(conversationKeys.messages('t1'))).toBeUndefined();
    });

    it('puts the chat back where it was when the delete is refused', async () => {
      queryClient.setQueryData(listKey, [{ id: 'c1' }, { id: 'c2' }]);
      queryClient.setQueryData([...conversationKeys.lists, 'empty'], undefined);
      conversationUiStore.setState({ activeConversationId: 'c1' } as never);
      h.api.deleteConversation.mockResolvedValueOnce(refused('conversation is busy'));
      const actions = setup();
      let deleted!: boolean;
      await act(async () => { deleted = await actions.deleteConversation('c1'); });
      expect(deleted).toBe(false);
      expect((queryClient.getQueryData(listKey) as Array<{ id: string }>).map((item) => item.id)).toEqual(['c1', 'c2']);
      expect(conversationUiStore.getState().activeConversationId).toBe('c1');
      expect(uiError()).toBe('conversation is busy');
    });

    it('does not reopen a chat the reader moved away from meanwhile', async () => {
      h.api.deleteConversation.mockImplementationOnce(async () => {
        conversationUiStore.setState({ activeConversationId: 'c2' } as never);
        return refused('x');
      });
      conversationUiStore.setState({ activeConversationId: 'c1' } as never);
      const actions = setup();
      await act(async () => { await actions.deleteConversation('c1'); });
      expect(conversationUiStore.getState().activeConversationId).toBe('c2');
    });
  });
});
