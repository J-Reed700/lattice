import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import { reportWarmupFailure } from '@/features/model/hooks/useDownloadedModels';
import { VaultAPI } from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';

import { ModelRolesProvider, useModelRoles } from './ModelRolesContext';

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

it('turns a failed model refresh into an error state instead of an unhandled rejection', async () => {
  api.getDownloadedModels = vi.fn().mockResolvedValue({ ok: false, error: 'database is locked' });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <ModelRolesProvider>{children}</ModelRolesProvider>
    </QueryClientProvider>
  );

  const { result } = renderHook(() => useModelRoles(), { wrapper });

  await waitFor(() => expect(result.current.isLoading).toBe(false));
  expect(result.current.error).toBe('database is locked');
  expect(result.current.localModels).toEqual([]);
});

it('clears the embedding role through the model client and shows why it failed', async () => {
  useToastStore.setState({ toasts: [] });
  api.getDownloadedModels = vi.fn().mockResolvedValue({ ok: true, data: [] });
  api.clearActiveEmbeddingModel = vi.fn().mockResolvedValue({ ok: false, error: 'index rebuild in progress' });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <ModelRolesProvider>{children}</ModelRolesProvider>
    </QueryClientProvider>
  );

  const { result } = renderHook(() => useModelRoles(), { wrapper });
  await waitFor(() => expect(result.current.isLoading).toBe(false));
  await act(() => result.current.assignRole(null, 'embedding'));

  expect(api.clearActiveEmbeddingModel).toHaveBeenCalledTimes(1);
  expect(useToastStore.getState().toasts).toEqual([
    expect.objectContaining({
      type: 'error',
      title: 'Failed to set embedding role',
      message: 'index rebuild in progress',
    }),
  ]);
});

it('tells the user when a model downloaded but its warm-up failed', () => {
  useToastStore.setState({ toasts: [] });
  reportWarmupFailure({ ok: false, error: 'out of memory' }, 'Qwen 3.5 4B', 'chat');
  reportWarmupFailure({ ok: true, data: undefined }, 'Qwen 3.5 4B', 'utility');
  expect(useToastStore.getState().toasts).toEqual([
    expect.objectContaining({ type: 'error', title: "Qwen 3.5 4B downloaded but couldn't start" }),
  ]);
});
