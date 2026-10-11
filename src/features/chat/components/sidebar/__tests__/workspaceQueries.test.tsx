import { type ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useCreateJournalMutation, useJournalsQuery, useSidebarBookmarksQuery } from '@/features/chat/components/sidebar/workspaceQueries';
import { VaultAPI } from '@/lib/api';
import { queryClient } from '@/lib/queryClient';


vi.mock('@/lib/api', () => ({ VaultAPI: { listJournals: vi.fn(), createJournal: vi.fn(), listMessageBookmarks: vi.fn() } }));

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return { client, wrapper };
}
const journal = (id: string, name: string) => ({ id, name, description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null, toolPreferencesJson: null, isArchived: false, sortOrder: 0, createdAt: '', updatedAt: '' });
const request = { name: 'New journal', description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null, toolPreferencesJson: null };

beforeEach(() => {
  vi.clearAllMocks();
});

describe('sidebar repository queries', () => {
  it('refreshes all journal readers from the repository after creation', async () => {
    let persisted = [journal('one', 'First')];
    vi.spyOn(VaultAPI, 'listJournals').mockImplementation(async () => ({ ok: true, data: persisted }));
    vi.spyOn(VaultAPI, 'createJournal').mockImplementation(async () => {
      const created = journal('two', 'New journal');
      persisted = [...persisted, created];
      return { ok: true, data: created };
    });
    const { wrapper } = setup();
    const { result } = renderHook(() => ({ first: useJournalsQuery(), second: useJournalsQuery(), create: useCreateJournalMutation() }), { wrapper });
    await waitFor(() => expect(result.current.first.journals).toHaveLength(1));
    await act(async () => { await result.current.create.mutateAsync(request); });
    await waitFor(() => {
      expect(result.current.first.journals.map(item => item.id)).toEqual(['one', 'two']);
      expect(result.current.second.journals).toEqual(persisted);
    });
  });

  it('does not replay a committed journal creation when its response fails', async () => {
    const defaults = queryClient.getDefaultOptions();
    const client = new QueryClient({ defaultOptions: { ...defaults, mutations: { ...defaults.mutations, retryDelay: 0 } } });
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
    const committed: string[] = [];
    vi.mocked(VaultAPI.createJournal).mockImplementation(async () => {
      committed.push(`journal-${committed.length + 1}`);
      return { ok: false, error: 'Response lost after the journal was saved' };
    });
    const { result } = renderHook(useCreateJournalMutation, { wrapper });
    await act(async () => {
      await expect(result.current.mutateAsync(request)).rejects.toThrow('Response lost');
    });
    expect(committed).toEqual(['journal-1']);
    expect(VaultAPI.createJournal).toHaveBeenCalledTimes(1);
    client.clear();
  });

  it('does not publish a journal when persistence rejects creation', async () => {
    vi.spyOn(VaultAPI, 'listJournals').mockResolvedValue({ ok: true, data: [journal('one', 'First')] });
    vi.spyOn(VaultAPI, 'createJournal').mockResolvedValue({ ok: false, error: 'Disk full' });
    const { wrapper } = setup();
    const { result } = renderHook(() => ({ read: useJournalsQuery(), create: useCreateJournalMutation() }), { wrapper });
    await waitFor(() => expect(result.current.read.journals).toHaveLength(1));
    await act(async () => { await expect(result.current.create.mutateAsync(request)).rejects.toThrow('Disk full'); });
    expect(result.current.read.journals.map(item => item.id)).toEqual(['one']);
  });

  it('does not let a late bookmark response replace a newer space selection', async () => {
    let resolveOld!: (value: Awaited<ReturnType<typeof VaultAPI.listMessageBookmarks>>) => void;
    vi.spyOn(VaultAPI, 'listMessageBookmarks')
      .mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; }))
      .mockResolvedValueOnce({ ok: true, data: { bookmarks: [], total: 0 } });
    const { wrapper } = setup();
    const { result, rerender } = renderHook(({ spaceId }) => useSidebarBookmarksQuery('', spaceId), { initialProps: { spaceId: 'old-space' }, wrapper });
    await waitFor(() => expect(VaultAPI.listMessageBookmarks).toHaveBeenCalledTimes(1));
    rerender({ spaceId: 'new-space' });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    await act(async () => { resolveOld({ ok: false, error: 'Old request failed' }); });
    expect(result.current.data).toEqual([]);
    expect(result.current.error).toBeNull();
  });
});
