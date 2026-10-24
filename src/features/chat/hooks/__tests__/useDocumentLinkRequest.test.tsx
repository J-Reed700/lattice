import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { fetchDocumentFileName, useLinkDocumentToSpaceMutation } from '@/features/chat/hooks/useDocumentLinkRequest';

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

describe('useDocumentLinkRequest', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    h.loadLinkedDocuments.mockResolvedValue(undefined);
  });

  it('puts a document in a space', async () => {
    h.api.setDocumentSpaceMembership.mockResolvedValueOnce({ ok: true, data: null });
    const { result } = renderHook(() => useLinkDocumentToSpaceMutation(), { wrapper });
    await act(async () => { await result.current.mutateAsync({ documentId: 'd1', spaceId: 's-hist' }); });
    expect(h.api.setDocumentSpaceMembership).toHaveBeenCalledWith('d1', 's-hist', true);
  });

  it('reads a document’s file name, or null', async () => {
    h.api.getDocument.mockResolvedValueOnce({ ok: true, data: { fileName: 'Gibbon vol 1.pdf' } });
    await expect(fetchDocumentFileName('d1')).resolves.toBe('Gibbon vol 1.pdf');
    h.api.getDocument.mockResolvedValueOnce({ ok: false, error: 'gone' });
    await expect(fetchDocumentFileName('d2')).resolves.toBeNull();
  });
});
