import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useConversationSourceFiling } from '@/features/chat/hooks/useConversationSourceFiling';

const h = vi.hoisted(() => ({
  api: {
    generateChatStarters: vi.fn(),
    addDocumentsToLibrary: vi.fn(),
    ingestWebUrl: vi.fn(),
    setDocumentSpaceMembership: vi.fn(),
    getDocument: vi.fn(),
  },
  loadLinkedDocuments: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ VaultAPI: h.api, default: h.api }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: unknown) => unknown) => select({ loadConversationLinkedDocuments: h.loadLinkedDocuments }),
}));

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

describe('useConversationSourceFiling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    h.loadLinkedDocuments.mockResolvedValue(undefined);
  });

  it('files documents into the library and refreshes the chat’s links only on success', async () => {
    const { result } = renderHook(() => useConversationSourceFiling('c1'), { wrapper });
    h.api.addDocumentsToLibrary.mockResolvedValueOnce({ ok: true, data: null });
    await act(async () => { await result.current.addDocumentsToLibrary(['d1']); });
    expect(h.api.addDocumentsToLibrary).toHaveBeenCalledWith(['d1']);
    expect(h.loadLinkedDocuments).toHaveBeenCalledWith('c1');
    h.api.addDocumentsToLibrary.mockResolvedValueOnce({ ok: false, error: 'x' });
    await act(async () => { await result.current.addDocumentsToLibrary(['d2']); });
    expect(h.loadLinkedDocuments).toHaveBeenCalledTimes(1);
  });

  it('indexes a web page for the chat, refreshing unless asked not to', async () => {
    const { result } = renderHook(() => useConversationSourceFiling('c1'), { wrapper });
    h.api.ingestWebUrl.mockResolvedValue({ ok: true, data: null });
    await act(async () => { await result.current.ingestWebSource({ url: 'https://example.org/rome', spaceId: 's-hist' }); });
    expect(h.api.ingestWebUrl).toHaveBeenCalledWith('https://example.org/rome', { conversationId: 'c1', spaceId: 's-hist' });
    expect(h.loadLinkedDocuments).toHaveBeenCalledTimes(1);
    await act(async () => { await result.current.ingestWebSource({ url: 'https://example.org/troy', skipRefresh: true }); });
    expect(h.loadLinkedDocuments).toHaveBeenCalledTimes(1);
    h.api.ingestWebUrl.mockResolvedValueOnce({ ok: false, error: 'HTTP 403' });
    await act(async () => { await result.current.ingestWebSource({ url: 'https://example.org/x' }); });
    expect(h.loadLinkedDocuments).toHaveBeenCalledTimes(1);
  });
});
