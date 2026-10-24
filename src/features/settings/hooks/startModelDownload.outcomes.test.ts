import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type { ModelMetadata } from '@/types/modelCatalog';

import { startModelDownload } from './startModelDownload';

const model = { id: 'gemma-2b', name: 'Gemma 2B', category: 'LLM', embedding_compatibility: null } as unknown as ModelMetadata;

describe('startModelDownload outcomes', () => {
  beforeEach(() => {
    vi.mocked(VaultAPI.downloadModel).mockReset();
  });

  it.each([
    ['started', { type: 'success', title: 'Download started', message: 'Gemma 2B' }],
    ['network_error', { type: 'error', title: "Couldn't reach the model host", message: 'Check your connection and try again.' }],
    ['failed', { type: 'error', title: "Couldn't start the download", message: 'The download could not be started.' }],
  ])('announces a %s download', async (status, toastArgs) => {
    vi.mocked(VaultAPI.downloadModel).mockResolvedValueOnce({ ok: true, data: { status } } as never);
    const addToast = vi.fn();
    await expect(startModelDownload({ metadata: model, addToast, queryClient: new QueryClient() })).resolves.toEqual({ alreadyDownloaded: false });
    expect(addToast).toHaveBeenCalledWith(toastArgs);
  });

  it('says a model already on disk is there and refreshes the downloaded list', async () => {
    vi.mocked(VaultAPI.downloadModel).mockResolvedValueOnce({ ok: true, data: { status: 'already_downloaded' } } as never);
    const queryClient = new QueryClient();
    vi.spyOn(queryClient, 'invalidateQueries');
    const addToast = vi.fn();
    await expect(startModelDownload({ metadata: model, addToast, queryClient })).resolves.toEqual({ alreadyDownloaded: true });
    expect(addToast).toHaveBeenCalledWith({ type: 'info', title: 'Already downloaded', message: 'Gemma 2B is on disk.' });
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['downloaded-models'] });
  });

  it('stays quiet for a status it does not know', async () => {
    vi.mocked(VaultAPI.downloadModel).mockResolvedValueOnce({ ok: true, data: { status: 'queued' } } as never);
    const addToast = vi.fn();
    await startModelDownload({ metadata: model, addToast, queryClient: new QueryClient() });
    expect(addToast).not.toHaveBeenCalled();
  });
});
