import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

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
  return renderHook(() => useDownloadedModels(), { wrapper });
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
});
