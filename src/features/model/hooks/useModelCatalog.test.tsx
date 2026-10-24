import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useModelCatalogStore } from '@/features/model/stores/modelCatalogStore';
import { toast } from '@/stores/toastStore';

import { useCompatibleModels, useModelCatalog, useModelSearch } from './useModelCatalog';

const api = vi.hoisted(() => ({
  detectSystemCapabilities: vi.fn(),
  getCompatibleModels: vi.fn(),
  getAllRecommendedModels: vi.fn(),
  searchModelCatalog: vi.fn(),
  getModelCatalogStats: vi.fn(),
  refreshModelCatalog: vi.fn(),
  clearModelCatalogCache: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const rec = (id: string, category: string) => ({ model: { id, category } });
const initialStore = useModelCatalogStore.getState();

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return { client, Wrapper: ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> };
}

beforeEach(() => {
  useModelCatalogStore.setState(initialStore, true);
  api.detectSystemCapabilities.mockReset().mockResolvedValue(ok({ total_ram_gb: 32 }));
  api.getCompatibleModels.mockReset().mockResolvedValue(ok([rec('embed-small', 'embedding'), rec('chat-9b', 'chat')]));
  api.getAllRecommendedModels.mockReset().mockResolvedValue(ok([rec('chat-9b', 'chat')]));
  api.searchModelCatalog.mockReset().mockResolvedValue(ok([{ id: 'qwen' }]));
  api.getModelCatalogStats.mockReset().mockResolvedValue(ok({ entries: 40 }));
  api.refreshModelCatalog.mockReset().mockResolvedValue(ok(null));
  api.clearModelCatalogCache.mockReset().mockResolvedValue(ok(null));
});
afterEach(() => vi.restoreAllMocks());

describe('useModelCatalog', () => {
  it('loads system capabilities and every recommended model by default', async () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelCatalog(), { wrapper: Wrapper });
    expect(result.current.isLoading).toBe(true);
    expect(result.current.isReady).toBe(false);
    await waitFor(() => expect(result.current.isReady).toBe(true));
    expect(result.current.systemCapabilities).toEqual({ total_ram_gb: 32 });
    await waitFor(() => expect(result.current.compatibleModels).toEqual([rec('chat-9b', 'chat')]));
    expect(api.getCompatibleModels).not.toHaveBeenCalled();
    expect(result.current.error).toBeNull();
    expect(result.current.cacheStats).toBeNull();
    expect(api.getModelCatalogStats).not.toHaveBeenCalled();
  });

  it('asks for a category’s compatible models and can load cache stats', async () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelCatalog({ category: 'embedding' as never, autoLoadCapabilities: false, loadCacheStats: true }), { wrapper: Wrapper });
    await waitFor(() => expect(result.current.cacheStats).toEqual({ entries: 40 }));
    expect(api.getCompatibleModels).toHaveBeenCalledWith('embedding');
    expect(api.detectSystemCapabilities).not.toHaveBeenCalled();
    await act(() => result.current.loadCacheStats());
    expect(api.getModelCatalogStats).toHaveBeenCalledTimes(2);
  });

  it('reports each failure and the first as the overall error', async () => {
    api.detectSystemCapabilities.mockResolvedValue({ ok: false, error: 'no GPU probe' });
    api.getAllRecommendedModels.mockResolvedValue({ ok: false, error: 'catalog missing' });
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelCatalog(), { wrapper: Wrapper });
    await waitFor(() => expect(result.current.compatibleModelsError).toBe('catalog missing'));
    expect(result.current.capabilitiesError).toBe('no GPU probe');
    expect(result.current.error).toBe('no GPU probe');
    expect(result.current.compatibleModels).toEqual([]);

    await act(() => result.current.loadAllModels());
    await waitFor(() => expect(result.current.allModelsError).toBe('catalog missing'));
  });

  it('searches with the current filters once typing settles', async () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelCatalog({ autoLoadCapabilities: false, autoLoadModels: false, searchDebounceMs: 5 }), { wrapper: Wrapper });
    act(() => {
      result.current.setFilters({ category: 'chat' as never, max_size_gb: 8 });
      result.current.search('  qwen ');
    });
    expect(result.current.searchQuery).toBe('  qwen ');
    expect(result.current.searchLoading).toBe(true);
    await waitFor(() => expect(result.current.searchResults).toEqual([{ id: 'qwen' }]));
    expect(api.searchModelCatalog).toHaveBeenCalledWith({ query: 'qwen', category: 'chat', max_size_gb: 8, required_capabilities: [], limit: 20 });
    expect(result.current.searchLoading).toBe(false);

    api.searchModelCatalog.mockResolvedValue({ ok: false, error: 'hub offline' });
    await expect(act(() => result.current.reloadSearch())).rejects.toThrow('hub offline');
    await waitFor(() => expect(result.current.searchError).toBe('hub offline'));
  });

  it('does not search a blank query', async () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelCatalog({ autoLoadCapabilities: false, autoLoadModels: false, searchDebounceMs: 1 }), { wrapper: Wrapper });
    act(() => result.current.setSearchQuery('   '));
    await waitFor(() => expect(result.current.searchLoading).toBe(false));
    expect(api.searchModelCatalog).not.toHaveBeenCalled();
    expect(result.current.searchResults).toEqual([]);
  });

  it('fetches on demand for imperative callers', async () => {
    const { Wrapper, client } = wrapper();
    const { result } = renderHook(() => useModelCatalog({ autoLoadCapabilities: false, autoLoadModels: false }), { wrapper: Wrapper });
    await act(() => result.current.loadSystemCapabilities());
    await waitFor(() => expect(result.current.systemCapabilities).toEqual({ total_ram_gb: 32 }));
    await act(() => result.current.loadCompatibleModels('chat' as never));
    expect(api.getCompatibleModels).toHaveBeenCalledWith('chat');
    await act(() => result.current.loadCompatibleModels());
    expect(client.getQueryData(['model-catalog', 'compatible', 'all'])).toEqual([rec('chat-9b', 'chat')]);
    await act(() => result.current.loadAllModels());
    await waitFor(() => expect(result.current.allModels).toEqual([rec('chat-9b', 'chat')]));
    const request = { query: 'mistral', limit: 5 } as never;
    act(() => result.current.debouncedSearch(request));
    await waitFor(() => expect(client.getQueryData(['model-catalog', 'search', request])).toEqual([{ id: 'qwen' }]));
  });

  it('refreshes the catalog and refetches what is on screen', async () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelCatalog(), { wrapper: Wrapper });
    await waitFor(() => expect(result.current.isReady).toBe(true));
    await act(() => result.current.refreshCatalog());
    await waitFor(() => expect(api.detectSystemCapabilities).toHaveBeenCalledTimes(2));
    await act(() => result.current.clearCache());
    expect(api.clearModelCatalogCache).toHaveBeenCalled();
    await waitFor(() => expect(api.detectSystemCapabilities).toHaveBeenCalledTimes(3));
  });

  it('warns that cached data is in use when a refresh fails', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const warning = vi.spyOn(toast, 'warning');
    api.refreshModelCatalog.mockResolvedValue({ ok: false, error: 'rate limited' });
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelCatalog({ autoLoadCapabilities: false, autoLoadModels: false }), { wrapper: Wrapper });
    await expect(act(() => result.current.refreshCatalog())).rejects.toThrow('rate limited');
    expect(warning).toHaveBeenCalledWith('Model catalog may be outdated', expect.objectContaining({ message: 'Using cached data. Try refreshing manually.' }));
  });

  it('keeps model selection in the store', () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelCatalog({ autoLoadCapabilities: false, autoLoadModels: false }), { wrapper: Wrapper });
    act(() => result.current.setSortBy('size' as never));
    expect(result.current.sortBy).toBe('size');
  });
});

describe('catalog convenience hooks', () => {
  it('useModelSearch only searches', async () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useModelSearch(1), { wrapper: Wrapper });
    act(() => result.current.search('phi'));
    await waitFor(() => expect(result.current.searchResults).toEqual([{ id: 'qwen' }]));
    expect(result.current.isSearching).toBe(false);
    expect(result.current.searchError).toBeNull();
    expect(api.detectSystemCapabilities).not.toHaveBeenCalled();
  });

  it('useCompatibleModels keeps only the asked-for category and can refresh', async () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useCompatibleModels('embedding' as never), { wrapper: Wrapper });
    await waitFor(() => expect(result.current.models).toEqual([rec('embed-small', 'embedding')]));
    expect(result.current.error).toBeNull();
    act(() => result.current.refresh());
    await waitFor(() => expect(api.getCompatibleModels).toHaveBeenCalledTimes(1));
  });

  it('useCompatibleModels without a category returns every recommendation', async () => {
    const { Wrapper } = wrapper();
    const { result } = renderHook(() => useCompatibleModels(), { wrapper: Wrapper });
    await waitFor(() => expect(result.current.models).toEqual([rec('chat-9b', 'chat')]));
    expect(result.current.isLoading).toBe(false);
  });
});
