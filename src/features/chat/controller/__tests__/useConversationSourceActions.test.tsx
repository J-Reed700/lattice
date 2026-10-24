import { QueryClient } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

import { useConversationSourceActions } from '../useConversationSourceActions';

const h = vi.hoisted(() => ({
  api: {
    listConversationLinkedDocuments: vi.fn(),
    listConversationWebSources: vi.fn(),
    listDocumentSpaceMemberships: vi.fn(),
    addConversationWebSource: vi.fn(),
    removeConversationWebSource: vi.fn(),
    removeConversationLinkedDocument: vi.fn(),
    setDocumentSpaceMembership: vi.fn(),
  },
}));
vi.mock('@/lib/api', () => ({ VaultAPI: h.api, default: h.api }));

let queryClient: QueryClient;
const ok = <T,>(data: T) => ({ ok: true as const, data });
const refused = (error: string) => ({ ok: false as const, error });

const setup = () => {
  const addRequestedId = vi.fn();
  const { result } = renderHook(() => useConversationSourceActions({ queryClient, addRequestedId }));
  return { actions: result.current, addRequestedId };
};

const uiError = () => conversationUiStore.getState().error;

describe('useConversationSourceActions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    conversationUiStore.setState({ error: null } as never);
  });

  it('loads linked documents and web sources, recording the request', async () => {
    h.api.listConversationLinkedDocuments.mockResolvedValueOnce(ok([{ documentId: 'd1' }]));
    h.api.listConversationWebSources.mockResolvedValueOnce(ok([{ id: 'w1' }]));
    const { actions, addRequestedId } = setup();
    await act(() => actions.loadConversationLinkedDocuments('c1'));
    await act(() => actions.loadConversationWebSources('c1'));
    expect(addRequestedId).toHaveBeenCalledWith('requestedLinkedConversationIds', 'c1');
    expect(addRequestedId).toHaveBeenCalledWith('requestedWebSourceConversationIds', 'c1');
    expect(queryClient.getQueryData(conversationKeys.linkedDocuments('c1'))).toEqual([{ documentId: 'd1' }]);
    expect(queryClient.getQueryData(conversationKeys.webSources('c1'))).toEqual([{ id: 'w1' }]);
  });

  it('shows a load failure instead of throwing', async () => {
    h.api.listConversationLinkedDocuments.mockResolvedValueOnce(refused('db locked'));
    const { actions } = setup();
    await act(() => actions.loadConversationLinkedDocuments('c1'));
    expect(uiError()).toBe('db locked');
    h.api.listConversationWebSources.mockResolvedValueOnce(refused('web table missing'));
    await act(() => actions.loadConversationWebSources('c1'));
    expect(uiError()).toBe('web table missing');
    h.api.listDocumentSpaceMemberships.mockResolvedValueOnce(refused('membership unreadable'));
    await act(() => actions.loadDocumentSpaceMemberships('d1'));
    expect(uiError()).toBe('membership unreadable');
  });

  it('links a web source and reloads the list', async () => {
    h.api.addConversationWebSource.mockResolvedValueOnce(ok(null));
    h.api.listConversationWebSources.mockResolvedValueOnce(ok([{ id: 'w2' }]));
    const { actions } = setup();
    let linked!: boolean;
    await act(async () => { linked = await actions.addConversationWebSource('c1', 'https://example.org/troy', { title: 'Troy' }); });
    expect(linked).toBe(true);
    expect(h.api.addConversationWebSource).toHaveBeenCalledWith('c1', 'https://example.org/troy', { title: 'Troy' });
    expect(queryClient.getQueryData(conversationKeys.webSources('c1'))).toEqual([{ id: 'w2' }]);
  });

  it('reports a link that was refused', async () => {
    h.api.addConversationWebSource.mockResolvedValueOnce(refused('bad url'));
    const { actions } = setup();
    let linked!: boolean;
    await act(async () => { linked = await actions.addConversationWebSource('c1', 'nope'); });
    expect(linked).toBe(false);
    expect(uiError()).toBe('bad url');
  });

  it('unlinks a web source from the cached list', async () => {
    queryClient.setQueryData(conversationKeys.webSources('c1'), [{ id: 'w1' }, { id: 'w2' }]);
    h.api.removeConversationWebSource.mockResolvedValueOnce(ok(null)).mockResolvedValueOnce(refused('locked'));
    const { actions } = setup();
    let removed!: boolean;
    await act(async () => { removed = await actions.removeConversationWebSource('c1', 'w1'); });
    expect(removed).toBe(true);
    expect(queryClient.getQueryData(conversationKeys.webSources('c1'))).toEqual([{ id: 'w2' }]);
    await act(async () => { removed = await actions.removeConversationWebSource('c1', 'w2'); });
    expect(removed).toBe(false);
    expect(uiError()).toBe('locked');
  });

  it('removes a linked document and forgets its memberships', async () => {
    queryClient.setQueryData(conversationKeys.linkedDocuments('c1'), [{ documentId: 'd1' }, { documentId: 'd2' }]);
    queryClient.setQueryData(conversationKeys.memberships('d1'), [{ spaceId: 's1' }]);
    h.api.removeConversationLinkedDocument.mockResolvedValueOnce(ok(null)).mockResolvedValueOnce(refused('in use'));
    const { actions } = setup();
    await act(() => actions.removeConversationLinkedDocument('c1', 'd1'));
    expect(queryClient.getQueryData(conversationKeys.linkedDocuments('c1'))).toEqual([{ documentId: 'd2' }]);
    expect(queryClient.getQueryData(conversationKeys.memberships('d1'))).toBeUndefined();
    await act(() => actions.removeConversationLinkedDocument('c1', 'd2'));
    expect(uiError()).toBe('in use');
    expect(queryClient.getQueryData(conversationKeys.linkedDocuments('c1'))).toEqual([{ documentId: 'd2' }]);
  });

  it('changes a document’s space and reloads its memberships', async () => {
    h.api.setDocumentSpaceMembership.mockResolvedValueOnce(ok(null)).mockResolvedValueOnce(refused('space archived'));
    h.api.listDocumentSpaceMemberships.mockResolvedValueOnce(ok([{ spaceId: 's-hist' }]));
    const { actions, addRequestedId } = setup();
    await act(() => actions.setDocumentSpaceMembership('d1', 's-hist', true));
    expect(h.api.setDocumentSpaceMembership).toHaveBeenCalledWith('d1', 's-hist', true);
    expect(addRequestedId).toHaveBeenCalledWith('requestedMembershipDocumentIds', 'd1');
    expect(queryClient.getQueryData(conversationKeys.memberships('d1'))).toEqual([{ spaceId: 's-hist' }]);
    await act(() => actions.setDocumentSpaceMembership('d1', 's-old', true));
    expect(uiError()).toBe('space archived');
  });
});
