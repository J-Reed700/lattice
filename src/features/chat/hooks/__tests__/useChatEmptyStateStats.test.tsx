import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useChatEmptyStateStats } from '@/features/chat/hooks/useChatEmptyStateStats';

const h = vi.hoisted(() => ({ api: { getIndexingStats: vi.fn(), listAllDocuments: vi.fn() } }));
vi.mock('@/lib/api', () => ({ default: h.api, VaultAPI: h.api }));

const NOW = new Date(2026, 4, 15, 12, 0, 0);
const at = (daysAgo: number) => new Date(NOW.getTime() - daysAgo * 86_400_000).toISOString();

const renderStats = () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return renderHook(() => useChatEmptyStateStats(), { wrapper });
};

describe('useChatEmptyStateStats', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('counts the library and what joined it today and this week', async () => {
    h.api.getIndexingStats.mockResolvedValueOnce({ ok: true, data: { indexedDocuments: 120 } });
    h.api.listAllDocuments.mockResolvedValueOnce({
      ok: true,
      data: [{ indexedAt: at(0) }, { indexedAt: at(2) }, { indexedAt: at(10) }, { indexedAt: 'not a date' }],
    });
    const { result } = renderStats();
    await waitFor(() => expect(result.current.data).toEqual({ totalDocuments: 120, documentsAddedToday: 1, documentsAddedThisWeek: 2 }));
    expect(h.api.listAllDocuments).toHaveBeenCalledWith(10000);
  });

  it('degrades to zeros when the readouts fail', async () => {
    h.api.getIndexingStats.mockResolvedValueOnce({ ok: false, error: 'db locked' });
    h.api.listAllDocuments.mockResolvedValueOnce({ ok: false, error: 'db locked' });
    const { result } = renderStats();
    await waitFor(() => expect(result.current.data).toEqual({ totalDocuments: 0, documentsAddedToday: 0, documentsAddedThisWeek: 0 }));
  });

  it('counts zero when the stats omit a total', async () => {
    h.api.getIndexingStats.mockResolvedValueOnce({ ok: true, data: {} });
    h.api.listAllDocuments.mockResolvedValueOnce({ ok: true, data: [] });
    const { result } = renderStats();
    await waitFor(() => expect(result.current.data?.totalDocuments).toBe(0));
  });
});
