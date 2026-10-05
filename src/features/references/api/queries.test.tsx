import { createElement, type ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor, cleanup } from '@testing-library/react';
import { beforeEach, afterEach, describe, it, expect, vi } from 'vitest';

import { useReferenceInbox } from '@/features/references/hooks/useReferenceInbox';
import { createConversationLifecycleRegistry } from '@/hooks/conversations/lifecycleRegistry';
import { useConversationActions } from '@/hooks/conversations/useConversationActions';
import { conversationKeys } from '@/hooks/queries/conversationKeys';
import { conversationUiStore } from '@/stores/conversationUiStore';

const api = vi.hoisted(() => ({
 listMessageBookmarks: vi.fn(), listWorkspaceNotes: vi.fn(), listConversationSpaces: vi.fn(), listJournals: vi.fn(),
 listPassageReferences: vi.fn(), getConversationMessages: vi.fn(), unbookmarkConversationMessage: vi.fn(),
 bookmarkConversationMessage: vi.fn(), updateWorkspaceNote: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));
const bookmark = (id = 'bookmark-one') => ({ id, conversationId: 'conv-one', conversationTitle: 'Conversation', spaceId: 'space_general', messageId: id, messageRole: 'assistant', messagePreview: 'Reference', title: null, note: null, createdAt: '2026-10-01T10:00:00.000Z' });
function harness(client: QueryClient) { return ({children}: {children: ReactNode}) => createElement(QueryClientProvider, {client}, children); }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(r => {resolve=r}); return {promise,resolve}; }
const clients: QueryClient[] = [];
function client() { const c = new QueryClient({ defaultOptions: { queries: {retry:false, gcTime:Infinity}, mutations: {retry:false} } }); clients.push(c); return c; }
beforeEach(() => {
 vi.resetAllMocks();
 api.listMessageBookmarks.mockResolvedValue({ok:true,data:{bookmarks:[bookmark()]}});
 api.listWorkspaceNotes.mockResolvedValue({ok:true,data:{notes:[]}});
 api.listConversationSpaces.mockResolvedValue({ok:true,data:[]});
 api.listJournals.mockResolvedValue({ok:true,data:[]});
 api.listPassageReferences.mockResolvedValue({ok:true,data:[]});
 api.getConversationMessages.mockResolvedValue({ok:false,error:'Unused source in audit probe'});
 api.unbookmarkConversationMessage.mockResolvedValue({ok:true,data:null});
 conversationUiStore.setState({error:null});
});
afterEach(() => { cleanup(); for(const c of clients.splice(0)) c.clear(); });
describe('Shared reference ownership', () => {
 it('removing a reference invalidates every bookmark consumer', async () => {
  const c = client(); const key=conversationKeys.bookmarks('conv-one'); c.setQueryData(key,[bookmark()]);
  const {result} = renderHook(() => useReferenceInbox({requestedReferenceId:null}),{wrapper:harness(c)});
  await waitFor(() => expect(result.current.bookmarks).toHaveLength(1));
  api.listMessageBookmarks.mockResolvedValue({ ok: true, data: { bookmarks: [] } });
  await act(async () => { expect(await result.current.removeReference(bookmark())).toBe(true); });
  expect(result.current.bookmarks).toHaveLength(0);
  expect(c.getQueryData(key)).toEqual([bookmark()]);
  expect(c.getQueryState(key)?.isInvalidated).toBe(true);
 });
 it('keeps the current search when an older request finishes late', async () => {
  const old=deferred<{ ok: boolean; data: { bookmarks: ReturnType<typeof bookmark>[] } }>(); const latest=deferred<{ ok: boolean; data: { bookmarks: ReturnType<typeof bookmark>[] } }>();
  api.listMessageBookmarks.mockImplementation((arg: { query?: string }) => arg.query==='new' ? latest.promise : old.promise);
  const {result}=renderHook(() => useReferenceInbox({requestedReferenceId:null}),{wrapper:harness(client())});
  await waitFor(() => expect(api.listMessageBookmarks).toHaveBeenCalledTimes(1));
  act(() => result.current.setQuery('new'));
  await waitFor(() => expect(api.listMessageBookmarks).toHaveBeenCalledTimes(2));
  await act(async () => { latest.resolve({ok:true,data:{bookmarks:[bookmark('new-result')]}}); });
  await waitFor(() => expect(result.current.bookmarks[0]?.id).toBe('new-result'));
  await act(async () => { old.resolve({ok:true,data:{bookmarks:[bookmark('old-result')]}}); });
  expect(result.current.bookmarks[0]?.id).toBe('new-result');
  expect(result.current.query).toBe('new');
 });
 it('returns failure to a view when its bookmark write is rejected', async () => {
  api.bookmarkConversationMessage.mockResolvedValue({ok:false,error:'Database unavailable'});
  const lifecycle=createConversationLifecycleRegistry();
  const {result}=renderHook(() => useConversationActions({queryClient:client(),addRequestedId:()=>{},lifecycle}));
  let saved: boolean | undefined;
  await act(async () => { saved=await result.current.bookmarkMessage('conv-one','msg-one'); });
  expect(saved).toBe(false);
  expect(conversationUiStore.getState().error).toBe('Database unavailable');
  lifecycle.cleanupAll();
 });
});
