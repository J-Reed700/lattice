import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import { reportWarmupFailure } from '@/features/model/hooks/useDownloadedModels';
import { VaultAPI } from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';
import { makeAppSettings } from '@/tests/fixtures/appSettings';

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

it('saves the Learning role as a model reference in Settings, and clears it the same way', async () => {
  useToastStore.setState({ toasts: [] });
  const settings = makeAppSettings();
  settings.llm.model = 'qwen3:8b';
  const rows = [
    { id: 'r1', model_id: 'qwen3.5-27b', model_name: 'Qwen3.5-27B', backend: 'local', model_type: 'chat' },
    { id: 'r2', model_id: '__ollama_server__', model_name: 'Ollama', backend: 'ollama', model_type: 'chat' },
  ];
  api.getDownloadedModels = vi.fn().mockResolvedValue({ ok: true, data: rows });
  api.getSettings = vi.fn().mockResolvedValue({ ok: true, data: settings });
  api.updateSettings = vi.fn().mockResolvedValue({ ok: true, data: settings });
  api.setActiveChatModel = vi.fn();
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <ModelRolesProvider>{children}</ModelRolesProvider>
    </QueryClientProvider>
  );

  const { result } = renderHook(() => useModelRoles(), { wrapper });
  await waitFor(() => expect(result.current.isLoading).toBe(false));
  await waitFor(() => expect(result.current.llmSettings).toBeDefined());

  await act(() => result.current.assignRole('qwen3.5-27b', 'learning'));
  expect(api.updateSettings).toHaveBeenLastCalledWith({
    category: 'llm', updates: { learningModel: { kind: 'local', model_id: 'qwen3.5-27b' } },
  });
  // The Ollama connection row names the chat tag served there.
  await act(() => result.current.assignRole('__ollama_server__', 'learning'));
  expect(api.updateSettings).toHaveBeenLastCalledWith({
    category: 'llm', updates: { learningModel: { kind: 'ollama', model: 'qwen3:8b' } },
  });
  await act(() => result.current.assignRole(null, 'learning'));
  expect(api.updateSettings).toHaveBeenLastCalledWith({ category: 'llm', updates: { learningModel: null } });
  // Learning is never a row flag and never the chat provider.
  expect(api.setActiveChatModel).not.toHaveBeenCalled();
  expect(useToastStore.getState().toasts).toEqual([]);
});
