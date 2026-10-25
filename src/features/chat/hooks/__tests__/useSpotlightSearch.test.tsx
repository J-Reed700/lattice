import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { useSpotlightSearch } from '@/features/chat/hooks/useSpotlightSearch';

const api = vi.hoisted(() => ({ listConversationsExplorer: vi.fn(), listMessageBookmarks: vi.fn() }));
vi.mock('@/lib/api', () => ({ VaultAPI: api, default: api }));

const chat = { id: 'c1', title: 'Canopy notes', explorerRoot: null };
const explorerThread = { id: 'c2', title: 'Folder thread', explorerRoot: '/notes' };
const bookmark = { conversationId: 'c1', messageId: 'm1' };

function renderSpotlight(initialProps: { query: string; isOpen: boolean }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return renderHook(({ query, isOpen }) => useSpotlightSearch(query, isOpen), { initialProps, wrapper });
}

beforeEach(() => {
  vi.clearAllMocks();
  api.listConversationsExplorer.mockResolvedValue({ ok: true, data: { conversations: [chat, explorerThread] } });
  api.listMessageBookmarks.mockResolvedValue({ ok: true, data: { bookmarks: [bookmark] } });
});

it('reads nothing while the spotlight is closed', () => {
  const { result } = renderSpotlight({ query: 'canopy', isOpen: false });
  expect(result.current).toEqual({ isLoading: false, conversations: [], bookmarks: [] });
  expect(api.listConversationsExplorer).not.toHaveBeenCalled();
});

it('searches chats and bookmarks with the trimmed query and leaves Explorer threads out', async () => {
  const { result } = renderSpotlight({ query: '  canopy ', isOpen: true });
  expect(result.current.isLoading).toBe(true);
  await waitFor(() => expect(result.current.isLoading).toBe(false));
  expect(api.listConversationsExplorer).toHaveBeenCalledWith({ query: 'canopy', includeArchived: true, limit: 20, offset: 0 });
  expect(api.listMessageBookmarks).toHaveBeenCalledWith({ query: 'canopy', limit: 20, offset: 0 });
  expect(result.current.conversations).toEqual([chat]);
  expect(result.current.bookmarks).toEqual([bookmark]);
});

it('shows a failed read as no results', async () => {
  api.listConversationsExplorer.mockResolvedValue({ ok: false, error: 'offline' });
  api.listMessageBookmarks.mockResolvedValue({ ok: false, error: 'offline' });
  const { result } = renderSpotlight({ query: '', isOpen: true });
  await waitFor(() => expect(result.current.isLoading).toBe(false));
  expect(result.current).toEqual({ isLoading: false, conversations: [], bookmarks: [] });
});

it('shows the last answer at once when it opens again, and reads it again behind it', async () => {
  const { result, rerender } = renderSpotlight({ query: 'canopy', isOpen: true });
  await waitFor(() => expect(result.current.conversations).toEqual([chat]));
  rerender({ query: 'canopy', isOpen: false });
  rerender({ query: 'canopy', isOpen: true });
  expect(result.current.isLoading).toBe(false);
  expect(result.current.conversations).toEqual([chat]);
  await waitFor(() => expect(api.listConversationsExplorer).toHaveBeenCalledTimes(2));
});
