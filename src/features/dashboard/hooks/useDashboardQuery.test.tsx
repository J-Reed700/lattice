import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useDashboardQuery } from '@/features/dashboard/hooks/useDashboardQuery';

const getIndexingStats = vi.fn();
const listAllDocuments = vi.fn();

vi.mock('@/lib/api', () => {
  const api = {
    getIndexingStats: () => getIndexingStats(),
    listAllDocuments: () => listAllDocuments(),
    getSystemStats: async () => ({ ok: true, data: { total_documents: 0, total_chunks: 0, total_tags: 0, storage_size_bytes: 0 } }),
    getIndexedFolders: async () => ({ ok: true, data: [] }),
    listConversations: async () => ({ ok: true, data: { conversations: [], total: 0 } }),
    listJournals: async () => ({ ok: true, data: [] }),
    listMessageBookmarks: async () => ({ ok: true, data: { bookmarks: [], total: 0 } }),
  };
  return { VaultAPI: api, default: api };
});

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

describe('useDashboardQuery failure', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getIndexingStats.mockResolvedValue({ ok: true, data: { indexedDocuments: 0, totalChunks: 0 } });
    listAllDocuments.mockResolvedValue({ ok: true, data: [] });
  });

  it('carries the backend reason when the library stats cannot be read', async () => {
    getIndexingStats.mockResolvedValue({ ok: false, error: 'The library database is locked.' });
    const { result } = renderHook(() => useDashboardQuery(), { wrapper });

    await waitFor(() => expect(result.current.error?.message).toBe('The library database is locked.'));
  });

  it('carries the backend reason when the document list cannot be read', async () => {
    listAllDocuments.mockResolvedValue({ ok: false, error: 'Disk read failed' });
    const { result } = renderHook(() => useDashboardQuery(), { wrapper });

    await waitFor(() => expect(result.current.error?.message).toBe('Disk read failed'));
  });
});
