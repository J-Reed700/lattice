import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';

import { useCustomCollectionActions } from './useCustomCollectionsQuery';

vi.mock('@/lib/api', () => ({ default: {
  listCustomCollections: vi.fn(),
  createCustomCollection: vi.fn(),
  renameCustomCollection: vi.fn(),
  moveCustomCollection: vi.fn(),
  deleteCustomCollection: vi.fn(),
  addDocumentsToCustomCollection: vi.fn(),
  removeDocumentsFromCustomCollection: vi.fn(),
} }));

const wrap = () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return { client, wrapper: ({ children }: { children: React.ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> };
};

describe('useCustomCollectionsQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(VaultAPI.listCustomCollections).mockResolvedValue({ ok: true, data: [] });
    vi.mocked(VaultAPI.createCustomCollection).mockResolvedValue({ ok: true, data: 'created' });
  });
  afterEach(() => vi.restoreAllMocks());

  it('creates concurrently without depending on a warmed query cache', async () => {
    const { wrapper } = wrap();
    const { result } = renderHook(() => useCustomCollectionActions(), { wrapper });
    await act(async () => {
      await Promise.all([result.current.create('First'), result.current.create('Second')]);
    });
    expect(VaultAPI.createCustomCollection).toHaveBeenCalledTimes(2);
  });
});
