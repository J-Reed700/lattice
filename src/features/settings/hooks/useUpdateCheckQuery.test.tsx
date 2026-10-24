import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useCheckForUpdatesMutation, useVersionInfoQuery } from './useUpdateCheckQuery';

const h = vi.hoisted(() => ({ api: { getVersionInfo: vi.fn(), checkForUpdates: vi.fn() } }));
vi.mock('@/lib/api', () => ({ default: h.api, VaultAPI: h.api }));

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

describe('useUpdateCheckQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  });

  it('reads the running version', async () => {
    h.api.getVersionInfo.mockResolvedValueOnce({ ok: true, data: { version: '0.9.1', commitHash: 'abc' } });
    const { result } = renderHook(() => useVersionInfoQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual({ version: '0.9.1', commitHash: 'abc' }));
  });

  it('surfaces a version it cannot read', async () => {
    h.api.getVersionInfo.mockResolvedValueOnce({ ok: false, error: 'no build info' });
    const { result } = renderHook(() => useVersionInfoQuery(), { wrapper });
    await waitFor(() => expect(result.current.error?.message).toBe('no build info'));
  });

  it('checks for updates only when asked', async () => {
    const { result } = renderHook(() => useCheckForUpdatesMutation(), { wrapper });
    expect(h.api.checkForUpdates).not.toHaveBeenCalled();
    h.api.checkForUpdates.mockResolvedValueOnce({ ok: true, data: { available: false } });
    await act(async () => { await result.current.mutateAsync(); });
    await waitFor(() => expect(result.current.data).toEqual({ available: false }));
    h.api.checkForUpdates.mockResolvedValueOnce({ ok: false, error: 'offline' });
    await act(async () => { await expect(result.current.mutateAsync()).rejects.toThrow('offline'); });
  });
});
