import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { runSynthesis } from '@/features/journal/synthesis/runSynthesis';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { RECENT_DECK_SIZE, useJournalSynthesis } from '../useJournalSynthesis';

vi.mock('@/features/journal/synthesis/runSynthesis', () => ({ runSynthesis: vi.fn() }));

const entries = Array.from({ length: 14 }, (_, i) => ({ id: `e${i}`, title: `Entry ${i}`, updatedAt: '2026-10-20T00:00:00Z', messageCount: 1 }));
const note = { id: 'page-1' } as WorkspaceNote;

function setup(overrides: Partial<Parameters<typeof useJournalSynthesis>[0]> = {}) {
  const client = new QueryClient();
  const options = {
    activeNote: note, entries, pinnedIds: new Set(['e3', 'e5']), selectedId: 'e2', notify: vi.fn(),
    searchParams: new URLSearchParams(), setSearchParams: vi.fn(), ...overrides,
  };
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const view = renderHook(() => useJournalSynthesis(options), { wrapper });
  return { ...options, client, synthesize: (scope: Parameters<ReturnType<typeof useJournalSynthesis>>[0]) => act(() => view.result.current(scope)) };
}

const lastRequest = () => vi.mocked(runSynthesis).mock.calls.at(-1)![0];

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date(2026, 9, 24));
  vi.mocked(runSynthesis).mockReset().mockResolvedValue(true);
});
afterEach(() => vi.useRealTimers());

describe('useJournalSynthesis', () => {
  it('synthesizes the selected entry into the open page under its own title', async () => {
    const s = setup();
    await s.synthesize('current');
    expect(lastRequest()).toEqual({
      title: 'Entry 2', heading: 'Current Entry', request: { conversationIds: ['e2'], scope: 'current' },
      destination: { kind: 'note', noteId: 'page-1' },
    });
    expect(vi.mocked(runSynthesis).mock.calls[0][1]).toBe(s.client);
  });

  it('uses the newest entry as current when none is selected', async () => {
    const s = setup({ selectedId: null });
    await s.synthesize('current');
    expect(lastRequest().request.conversationIds).toEqual(['e0']);
  });

  it('synthesizes pinned entries and the recent deck', async () => {
    const s = setup();
    await s.synthesize('pinned');
    expect(lastRequest()).toMatchObject({ title: 'Pinned Entries', request: { conversationIds: ['e3', 'e5'] } });
    await s.synthesize('deck');
    expect(lastRequest().request.conversationIds).toHaveLength(RECENT_DECK_SIZE);
  });

  it('sends the past week to the week’s own page, selected by the backend', async () => {
    const s = setup();
    await s.synthesize('week');
    expect(lastRequest()).toEqual({
      title: 'Past Week', heading: 'Past Week', request: { conversationIds: [], scope: 'week' },
      destination: { kind: 'week', title: 'Week of Oct 19' },
    });
  });

  it('refuses without a page or without entries', async () => {
    const noPage = setup({ activeNote: null });
    await expect(noPage.synthesize('deck')).resolves.toBe(false);
    expect(noPage.notify).toHaveBeenCalledWith('error', 'Select a notebook page before synthesizing.');

    const empty = setup({ entries: [], selectedId: null, pinnedIds: new Set() });
    await empty.synthesize('current');
    await empty.synthesize('pinned');
    await empty.synthesize('conversation');
    expect(empty.notify).toHaveBeenCalledTimes(3);
    expect(empty.notify).toHaveBeenCalledWith('error', 'No journal entries available for synthesis.');
    expect(runSynthesis).not.toHaveBeenCalled();
  });

  it('answers Home’s week link once a page is open, removing the flag', () => {
    const searchParams = new URLSearchParams('synthesize=week&journalSpaceId=j1');
    const s = setup({ searchParams });
    expect(s.setSearchParams).toHaveBeenCalledWith(new URLSearchParams('journalSpaceId=j1'), { replace: true });
    expect(lastRequest().request.scope).toBe('week');
  });

  it('waits for a page before answering the week link', () => {
    const s = setup({ searchParams: new URLSearchParams('synthesize=week'), activeNote: null });
    expect(s.setSearchParams).not.toHaveBeenCalled();
    expect(runSynthesis).not.toHaveBeenCalled();
  });
});
