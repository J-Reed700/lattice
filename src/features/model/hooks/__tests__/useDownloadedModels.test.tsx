import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useCloudApiKeyMutation } from '@/features/settings/hooks/useSettingsQuery';
import { VaultAPI } from '@/lib/api';
import type { DownloadedModel } from '@/types/downloadedModels';

import { useDownloadedModels } from '../useDownloadedModels';


const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

function renderModels() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  return { queryClient, ...renderHook(() => useDownloadedModels(), { wrapper }) };
}

describe('useDownloadedModels', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
    api.getDownloadedModels = vi.fn().mockResolvedValue({
      ok: true,
      data: [{ id: 'row-1', model_id: 'qwen3-4b' } as unknown as DownloadedModel],
    });
  });

  it('reports a refused delete instead of retrying the command', async () => {
    api.deleteDownloadedModel = vi.fn().mockResolvedValue({ ok: false, error: 'model is loaded' });
    const { result } = renderModels();

    await expect(result.current.deleteDownloadedModel('row-1')).rejects.toThrow('model is loaded');
    expect(api.deleteDownloadedModel).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('answers a failed download check from the models already loaded', async () => {
    api.isModelDownloaded = vi.fn().mockResolvedValue({ ok: false, error: 'database is locked' });
    const { result } = renderModels();
    await waitFor(() => expect(result.current.downloadedModels).toHaveLength(1));

    await expect(result.current.isModelDownloaded('qwen3-4b')).resolves.toBe(true);
    await expect(result.current.isModelDownloaded('missing')).resolves.toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('invalidates settings, active-models, and course index queries on embedding change and model delete', async () => {
    api.setActiveEmbeddingModel = vi.fn().mockResolvedValue({ ok: true, data: undefined });
    api.deleteDownloadedModel = vi.fn().mockResolvedValue({ ok: true, data: undefined });
    const { result, queryClient } = renderModels();
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

    await act(() => result.current.setActiveEmbeddingModel('row-1'));
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['downloaded-models'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['settings'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['active-models'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['learning', 'course-index'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['course-index-status'] });

    invalidateSpy.mockClear();
    await act(() => result.current.deleteDownloadedModel('row-1'));
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['downloaded-models'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['settings'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['active-models'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['learning', 'course-index'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['course-index-status'] });
  });

  it('invalidates settings, active-models, and course index queries when saving a cloud API key', async () => {
    api.setCloudApiKey = vi.fn().mockResolvedValue({ ok: true, data: undefined });
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useCloudApiKeyMutation(), { wrapper });

    await act(() => result.current.setCloudApiKey({ provider: 'openai', key: 'sk-test' }));
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['settings'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['active-models'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['learning', 'course-index'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['course-index-status'] });
  });
});
