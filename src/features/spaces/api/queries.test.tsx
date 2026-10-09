import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationKeys } from '@/hooks/queries/conversationKeys';

import { spaceKeys, useSpaceMutations, useSpacesQuery } from './queries';

const api = vi.hoisted(() => ({
  listConversationSpaces: vi.fn(),
  createConversationSpace: vi.fn(),
  updateConversationSpace: vi.fn(),
  archiveConversationSpace: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

const space = (id: string, name: string, isArchived = false) => ({
  id, name, description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null,
  toolPreferencesJson: null, isArchived, sortOrder: 0, createdAt: '2026-10-09T10:00:00Z', updatedAt: '2026-10-09T10:00:00Z',
});

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

beforeEach(() => {
  vi.clearAllMocks();
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
});

describe('the spaces query', () => {
  it('is one cache entry every view shares', async () => {
    api.listConversationSpaces.mockResolvedValue({ ok: true, data: [space('space_general', 'General')] });
    const first = renderHook(() => useSpacesQuery(), { wrapper });
    const second = renderHook(() => useSpacesQuery(), { wrapper });
    await waitFor(() => expect(first.result.current.data).toHaveLength(1));
    await waitFor(() => expect(second.result.current.data).toHaveLength(1));
    expect(api.listConversationSpaces).toHaveBeenCalledOnce();
    expect(client.getQueryData(spaceKeys.list)).toEqual([space('space_general', 'General')]);
  });

  it('reports a backend failure as an error', async () => {
    api.listConversationSpaces.mockResolvedValue({ ok: false, error: 'Database busy' });
    const { result } = renderHook(() => useSpacesQuery(), { wrapper });
    await waitFor(() => expect(result.current.error?.message).toBe('Database busy'));
  });
});

describe('space mutations', () => {
  it('a created space shows in the list without a reload', async () => {
    api.listConversationSpaces.mockResolvedValueOnce({ ok: true, data: [space('space_general', 'General')] });
    const list = renderHook(() => useSpacesQuery(), { wrapper });
    await waitFor(() => expect(list.result.current.data).toHaveLength(1));

    api.createConversationSpace.mockResolvedValue({ ok: true, data: space('space_movies', 'Movies') });
    api.listConversationSpaces.mockResolvedValueOnce({ ok: true, data: [space('space_general', 'General'), space('space_movies', 'Movies')] });
    const { result } = renderHook(() => useSpaceMutations(), { wrapper });
    await act(async () => {
      await result.current.create.mutateAsync({
        name: 'Movies', description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null, toolPreferencesJson: null,
      });
    });
    // Resolving waits for the refreshed list, so the caller can select the new space at once.
    expect(client.getQueryData<Array<{ name: string }>>(spaceKeys.list)?.map((item) => item.name)).toEqual(['General', 'Movies']);
    await waitFor(() => expect(list.result.current.data).toHaveLength(2));
  });

  it('an edit refreshes spaces but not conversations', async () => {
    client.setQueryData(spaceKeys.list, [space('space_movies', 'Movies')]);
    client.setQueryData(conversationKeys.lists, []);
    api.updateConversationSpace.mockResolvedValue({ ok: true, data: space('space_movies', 'Films') });
    const { result } = renderHook(() => useSpaceMutations(), { wrapper });
    await act(async () => {
      await result.current.update.mutateAsync({
        spaceId: 'space_movies', name: 'Films', description: null, icon: null, accentColor: null,
        defaultModelName: null, spacePrompt: null, toolPreferencesJson: null,
      });
    });
    expect(client.getQueryState(spaceKeys.list)?.isInvalidated).toBe(true);
    expect(client.getQueryState(conversationKeys.lists)?.isInvalidated).toBe(false);
  });

  it('archiving refreshes spaces and the conversation lists that leave its chats out', async () => {
    client.setQueryData(spaceKeys.list, [space('space_movies', 'Movies')]);
    client.setQueryData(conversationKeys.lists, []);
    api.archiveConversationSpace.mockResolvedValue({ ok: true, data: { status: 'success' } });
    const { result } = renderHook(() => useSpaceMutations(), { wrapper });
    await act(async () => {
      await result.current.archive.mutateAsync({ spaceId: 'space_movies', archived: true });
    });
    expect(client.getQueryState(spaceKeys.list)?.isInvalidated).toBe(true);
    expect(client.getQueryState(conversationKeys.lists)?.isInvalidated).toBe(true);
  });

  it('a refused write rejects with the backend message and leaves the cache alone', async () => {
    client.setQueryData(spaceKeys.list, [space('space_general', 'General')]);
    api.archiveConversationSpace.mockResolvedValue({ ok: false, error: 'The General space cannot be archived' });
    const { result } = renderHook(() => useSpaceMutations(), { wrapper });
    await act(async () => {
      await expect(result.current.archive.mutateAsync({ spaceId: 'space_general', archived: true }))
        .rejects.toThrow('The General space cannot be archived');
    });
    expect(client.getQueryState(spaceKeys.list)?.isInvalidated).toBe(false);
  });
});
