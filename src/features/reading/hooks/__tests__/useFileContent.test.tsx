import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, type RenderHookOptions, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import { deferred } from '@/tests/deferred';
import type { ApiResult } from '@/types';

import { useFileContent } from '../useFileContent';

vi.mock('@/lib/api', () => {
  const api = { readFileContent: vi.fn() };
  return { default: api, VaultAPI: api };
});

function renderWithClient<Result, Props>(hook: (props: Props) => Result, options?: RenderHookOptions<Props>) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return renderHook(hook, { ...options, wrapper });
}
beforeEach(() => { vi.mocked(VaultAPI.readFileContent).mockReset(); });
afterEach(() => { vi.restoreAllMocks(); });

it.each([undefined, ''])('does not read an absent path (%s)', filePath => {
  const { result } = renderWithClient(() => useFileContent(filePath));
  expect(result.current).toEqual({ content: '', isLoading: false, error: null });
  expect(VaultAPI.readFileContent).not.toHaveBeenCalled();
});

it('does not load until enabled, including empty files and Unicode paths', async () => {
  vi.mocked(VaultAPI.readFileContent).mockResolvedValue({ ok: true, data: '' });
  const { result, rerender } = renderWithClient(({ enabled }) => useFileContent('/café/空白.md', enabled), { initialProps: { enabled: false } });
  expect(VaultAPI.readFileContent).not.toHaveBeenCalled();
  rerender({ enabled: true });
  await waitFor(() => expect(result.current.isLoading).toBe(false));
  expect(result.current).toEqual({ content: '', isLoading: false, error: null });
  expect(VaultAPI.readFileContent).toHaveBeenCalledWith('/café/空白.md');
});

it.each(['success', 'failure'] as const)('ignores stale %s after switching files', async kind => {
  const old = deferred<ApiResult<string>>();
  const current = deferred<ApiResult<string>>();
  vi.mocked(VaultAPI.readFileContent).mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
  const { result, rerender } = renderWithClient(({ path }) => useFileContent(path), { initialProps: { path: '/old.md' } });
  rerender({ path: '/current.md' });
  await act(async () => { old.resolve(kind === 'success' ? { ok: true, data: 'Wrong file' } : { ok: false, error: 'Old failure' }); });
  expect(result.current).toEqual({ content: '', isLoading: true, error: null });
  await act(async () => { current.resolve({ ok: true, data: 'Current file' }); });
  await waitFor(() => expect(result.current).toEqual({ content: 'Current file', isLoading: false, error: null }));
});

it('clears content while changing paths and discards a response after disabling', async () => {
  const pending = deferred<ApiResult<string>>();
  vi.mocked(VaultAPI.readFileContent).mockResolvedValueOnce({ ok: true, data: 'Saved file' }).mockReturnValueOnce(pending.promise);
  const { result, rerender } = renderWithClient(({ path, enabled }) => useFileContent(path, enabled), { initialProps: { path: '/first', enabled: true } });
  await waitFor(() => expect(result.current.content).toBe('Saved file'));
  rerender({ path: '/second', enabled: true });
  expect(result.current.content).toBe('');
  rerender({ path: '/second', enabled: false });
  await act(async () => { pending.resolve({ ok: true, data: 'Too late' }); });
  expect(result.current).toEqual({ content: '', isLoading: false, error: null });
});

it.each([
  [new Error('Permission denied'), 'Permission denied'],
  ['transport disconnected', 'Failed to load file'],
])('reports rejected reads and recovers on a new path (%s)', async (error, message) => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.mocked(VaultAPI.readFileContent).mockRejectedValueOnce(error).mockResolvedValueOnce({ ok: true, data: 'Recovered' });
  const { result, rerender } = renderWithClient(({ path }) => useFileContent(path), { initialProps: { path: '/first' } });
  await waitFor(() => expect(result.current.error).toBe(message));
  expect(result.current.isLoading).toBe(false);
  rerender({ path: '/second' });
  await waitFor(() => expect(result.current.content).toBe('Recovered'));
  expect(result.current.error).toBeNull();
});

it('handles structured read failures and late rejection after unmount', async () => {
  const late = deferred<ApiResult<string>>();
  const logged = vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.mocked(VaultAPI.readFileContent).mockResolvedValueOnce({ ok: false, error: 'Missing file' }).mockReturnValueOnce(late.promise);
  const { result, rerender, unmount } = renderWithClient(({ path }) => useFileContent(path), { initialProps: { path: '/missing' } });
  await waitFor(() => expect(result.current.error).toBe('Missing file'));
  logged.mockClear();
  rerender({ path: '/unmounted' });
  unmount();
  await act(async () => { late.reject(new Error('Late read')); });
  expect(logged).not.toHaveBeenCalled();
});
