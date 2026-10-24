import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { chatStartersQueryKey, useChatStartersQuery } from '@/features/chat/hooks/useChatStartersQuery';

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

describe('useChatStartersQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    h.loadLinkedDocuments.mockResolvedValue(undefined);
  });

  it('asks for starters for a space and keys them by it', async () => {
    const starters = { fingerprint: 'f', generatedAt: 'now', starters: [{ question: 'What did the Romans eat?' }], documentCount: 4 };
    h.api.generateChatStarters.mockResolvedValueOnce({ ok: true, data: starters });
    const { result } = renderHook(() => useChatStartersQuery('s-hist'), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual(starters));
    expect(h.api.generateChatStarters).toHaveBeenCalledWith('s-hist');
    expect(chatStartersQueryKey('s-hist')).toEqual(['chat', 'starters', 's-hist']);
  });

  it('offers no starters when they cannot be made', async () => {
    h.api.generateChatStarters.mockResolvedValueOnce({ ok: false, error: 'no model' });
    const { result } = renderHook(() => useChatStartersQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual({ fingerprint: '', generatedAt: '', starters: [], documentCount: 0 }));
    expect(h.api.generateChatStarters).toHaveBeenCalledWith(null);
  });
});
