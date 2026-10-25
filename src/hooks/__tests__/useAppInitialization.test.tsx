import type { ReactNode } from 'react';

import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import { createTestApi } from '@/tests/apiClient';
import { deferred } from '@/tests/deferred';
import type { ApiResult } from '@/types';

import { useAppInitialization } from '../useAppInitialization';

describe('useAppInitialization', () => {
  it('waits for the bridge before becoming ready', async () => {
    const pending = deferred<ApiResult<string>>();
    const api = createTestApi({ initializeDatabase: vi.fn(() => pending.promise) });
    const wrapper = ({ children }: { children: ReactNode }) => <ApiProvider client={api}>{children}</ApiProvider>;
    const { result } = renderHook(useAppInitialization, { wrapper });
    expect(result.current).toEqual({ status: 'pending' });
    await act(async () => { pending.resolve({ ok: true, data: 'Ready' }); });
    expect(result.current).toEqual({ status: 'ready' });
  });

  it.each([
    ['a failed command', () => Promise.resolve({ ok: false as const, error: 'Bridge unavailable' })],
    ['a rejected request', () => Promise.reject(new Error('Bridge unavailable'))],
  ])('reports %s', async (_label, initializeDatabase) => {
    const api = createTestApi({ initializeDatabase: vi.fn(initializeDatabase) });
    const wrapper = ({ children }: { children: ReactNode }) => <ApiProvider client={api}>{children}</ApiProvider>;
    const { result } = renderHook(useAppInitialization, { wrapper });
    await waitFor(() => expect(result.current).toEqual({ status: 'error', error: 'Bridge unavailable' }));
  });

  it('ignores a disposed request when Strict Mode replays the effect', async () => {
    const previous = deferred<ApiResult<string>>();
    const initializeDatabase = vi.fn().mockReturnValueOnce(previous.promise).mockResolvedValue({ ok: true, data: 'Ready' });
    const api = createTestApi({ initializeDatabase });
    const wrapper = ({ children }: { children: ReactNode }) => <ApiProvider client={api}>{children}</ApiProvider>;
    const { result } = renderHook(useAppInitialization, { wrapper, reactStrictMode: true });
    await waitFor(() => expect(result.current.status).toBe('ready'));
    await act(async () => { previous.reject(new Error('Stale failure')); });
    expect(result.current).toEqual({ status: 'ready' });
    expect(initializeDatabase).toHaveBeenCalledTimes(2);
  });

  it('allows a pending request to settle after unmount', async () => {
    const pending = deferred<ApiResult<string>>();
    const api = createTestApi({ initializeDatabase: vi.fn(() => pending.promise) });
    const wrapper = ({ children }: { children: ReactNode }) => <ApiProvider client={api}>{children}</ApiProvider>;
    const view = renderHook(useAppInitialization, { wrapper });
    view.unmount();
    await act(async () => { pending.resolve({ ok: true, data: 'Ready' }); });
    expect(view.result.current).toEqual({ status: 'pending' });
  });
});
