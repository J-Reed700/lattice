import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import { createTestApi } from '@/tests/apiClient';

import { useRemoveDownloadedModel } from './useRemoveDownloadedModel';

const STATUS_KEY = ['speech', 'status'] as const;
const record = (id: string, model_id: string) => ({ id, model_id });

function setup(models: unknown[], deleteResult: unknown = { ok: true, data: undefined }) {
  const api = createTestApi({
    getDownloadedModels: vi.fn(async () => ({ ok: true as const, data: models })) as never,
    deleteDownloadedModel: vi.fn(async () => deleteResult) as never,
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const invalidated: unknown[] = [];
  const original = client.invalidateQueries.bind(client);
  vi.spyOn(client, 'invalidateQueries').mockImplementation(async (filters) => {
    invalidated.push(filters?.queryKey);
    await Promise.resolve();
    return original(filters);
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <ApiProvider client={api}>
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    </ApiProvider>
  );
  const { result } = renderHook(() => useRemoveDownloadedModel(), { wrapper });
  return { api, invalidated, remove: result.current };
}

describe('useRemoveDownloadedModel', () => {
  it('deletes by record id, with its files', async () => {
    const { api, remove } = setup([record('rec-1', 'other'), record('rec-2', 'kokoro-82m-more-voices')]);
    await remove('kokoro-82m-more-voices', STATUS_KEY);
    expect(api.deleteDownloadedModel).toHaveBeenCalledTimes(1);
    expect(api.deleteDownloadedModel).toHaveBeenCalledWith('rec-2', true);
  });

  it('refreshes the download list before the status', async () => {
    const { invalidated, remove } = setup([record('rec-1', 'm')]);
    await remove('m', STATUS_KEY);
    expect(invalidated[0]).toEqual(['downloads']);
    expect(invalidated.slice(1)).toEqual(expect.arrayContaining([['downloaded-models'], STATUS_KEY]));
  });

  it('skips the delete when the record is already gone, and still refreshes', async () => {
    const { api, invalidated, remove } = setup([]);
    await remove('m', STATUS_KEY);
    expect(api.deleteDownloadedModel).not.toHaveBeenCalled();
    expect(invalidated).toContainEqual(STATUS_KEY);
  });

  it('rejects with the backend message and leaves the status alone when the delete fails', async () => {
    const { invalidated, remove } = setup([record('rec-1', 'm')], { ok: false, error: 'Files are in use.' });
    await expect(remove('m', STATUS_KEY)).rejects.toThrow('Files are in use.');
    expect(invalidated).not.toContainEqual(STATUS_KEY);
  });
});
