import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useCloudApiKeyMutation } from '@/features/settings/hooks/useSettingsQuery';
import { deferred } from '@/tests/deferred';

import { useCloudModelCatalogs, useCloudModels } from './useCloudModels';

const api = vi.hoisted(() => ({ listCloudModels: vi.fn(), setCloudApiKey: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api }));

const catalog = (id: string) => ({ hasApiKey: true, models: [{ id, name: id }] });
const ok = <T,>(data: T) => ({ ok: true as const, data });
const setup = () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return { client, wrapper: ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> };
};

beforeEach(() => {
  api.listCloudModels.mockReset().mockImplementation(async () => ok({ hasApiKey: false, models: [] }));
  api.setCloudApiKey.mockReset().mockResolvedValue(ok(undefined));
});

describe('cloud model catalogs', () => {
  it('loads providers independently and keeps successful lists when one key is rejected', async () => {
    api.listCloudModels.mockImplementation(async (provider: string) => provider === 'anthropic'
      ? { ok: false, error: 'Anthropic API key was rejected' }
      : ok(provider === 'openai' ? catalog('gpt-new') : { hasApiKey: false, models: [] }));
    const { wrapper } = setup();
    const { result } = renderHook(() => useCloudModelCatalogs(), { wrapper });
    await waitFor(() => expect(result.current.isFetching).toBe(false));
    expect(result.current.catalogs.openai?.models[0].id).toBe('gpt-new');
    expect(result.current.errors).toEqual([{ provider: 'anthropic', message: 'Anthropic API key was rejected' }]);
    expect(result.current.catalogs.google?.hasApiKey).toBe(false);
  });

  it('shares cached models and refreshes explicitly', async () => {
    api.listCloudModels.mockResolvedValue(ok(catalog('gpt-old')));
    const { wrapper } = setup();
    const first = renderHook(() => useCloudModels('openai'), { wrapper });
    await waitFor(() => expect(first.result.current.data?.models[0].id).toBe('gpt-old'));
    const second = renderHook(() => useCloudModels('openai'), { wrapper });
    expect(second.result.current.data?.models[0].id).toBe('gpt-old');
    expect(api.listCloudModels).toHaveBeenCalledTimes(1);
    api.listCloudModels.mockResolvedValue(ok(catalog('gpt-new')));
    await act(async () => { await first.result.current.refetch(); });
    await waitFor(() => expect(second.result.current.data?.models[0].id).toBe('gpt-new'));
  });

  it('discards an old key’s in-flight response when a replacement key is saved', async () => {
    const old = deferred<ReturnType<typeof ok<ReturnType<typeof catalog>>>>();
    api.listCloudModels.mockReturnValueOnce(old.promise).mockResolvedValue(ok(catalog('gpt-new')));
    const { wrapper } = setup();
    const { result } = renderHook(() => ({ query: useCloudModels('openai'), mutation: useCloudApiKeyMutation() }), { wrapper });
    await waitFor(() => expect(api.listCloudModels).toHaveBeenCalledTimes(1));
    await act(async () => { await result.current.mutation.setCloudApiKey({ provider: 'openai', key: 'replacement' }); });
    await waitFor(() => expect(result.current.query.data?.models[0].id).toBe('gpt-new'));
    await act(async () => { old.resolve(ok(catalog('gpt-old'))); await old.promise; });
    expect(result.current.query.data?.models[0].id).toBe('gpt-new');
  });
});
