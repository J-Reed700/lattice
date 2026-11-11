import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';

import { REMOTE_AUTH_STATUS_QUERY_KEY } from './useRemoteAuthStatus';
import { useSettingsTransfer } from './useSettingsQuery';

const api = { resetSettings: vi.fn(), importSettings: vi.fn() };

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={client}>
    <ApiProvider client={api as unknown as ApiClient}>{children}</ApiProvider>
  </QueryClientProvider>
);

describe('useSettingsTransfer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  });

  it('refreshes the stored-value status after a reset', async () => {
    api.resetSettings.mockResolvedValueOnce({ ok: true, data: {} });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const { result } = renderHook(() => useSettingsTransfer(), { wrapper });
    await act(async () => { await result.current.resetSettings(); });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: REMOTE_AUTH_STATUS_QUERY_KEY });
  });

  it('refreshes the stored-value status after an import', async () => {
    api.importSettings.mockResolvedValueOnce({ ok: true, data: {} });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const { result } = renderHook(() => useSettingsTransfer(), { wrapper });
    await act(async () => { await result.current.importSettings('{}'); });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: REMOTE_AUTH_STATUS_QUERY_KEY });
  });

  it('leaves the stored-value status alone when the reset fails', async () => {
    api.resetSettings.mockResolvedValueOnce({ ok: false, error: 'nope' });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const { result } = renderHook(() => useSettingsTransfer(), { wrapper });
    await act(async () => { await result.current.resetSettings(); });
    expect(invalidate).not.toHaveBeenCalledWith({ queryKey: REMOTE_AUTH_STATUS_QUERY_KEY });
  });
});
