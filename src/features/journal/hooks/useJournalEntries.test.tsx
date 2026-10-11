import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';
import { useVaultImportStore } from '@/stores/vaultImportStore';
import { deferred } from '@/tests/deferred';

import { useJournalEntries } from './useJournalEntries';

vi.mock('@/lib/api', () => ({ default: { listJournalConversations: vi.fn(), getConversationMessages: vi.fn() } }));
type ListResult = Awaited<ReturnType<typeof VaultAPI.listJournalConversations>>;
const response = (conversations: unknown): ListResult => ({ ok: true, data: { conversations } } as ListResult);
const first = { id: 'a', title: 'Older café entry', updatedAt: '2026-10-07T12:00:00Z', messageCount: 1 };
const second = { id: 'b', title: 'Latest entry', updatedAt: '2026-10-08T12:00:00Z', messageCount: 2 };
const initial = { journalSpaceId: 'journal-a' as string | null, requestedEntryId: null as string | null };
const withoutJournal: typeof initial = { ...initial, journalSpaceId: null };

beforeEach(() => {
  vi.resetAllMocks(); localStorage.clear();
  useVaultImportStore.setState({ importTick: 0, lastImportedId: null });
  vi.mocked(VaultAPI.listJournalConversations).mockResolvedValue(response([first, second]));
  vi.mocked(VaultAPI.getConversationMessages).mockResolvedValue({ ok: true, data: { messages: [], total: 0 } });
});
afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); localStorage.clear(); });

it('does not query without a journal and clears entries when leaving it', async () => {
  const { result, rerender } = renderHook(useJournalEntries, { initialProps: withoutJournal });
  expect(result.current.isLoading).toBe(false);
  expect(VaultAPI.listJournalConversations).not.toHaveBeenCalled();
  rerender(initial);
  await waitFor(() => expect(result.current.entries).toHaveLength(2));
  rerender(withoutJournal);
  expect(result.current.entries).toEqual([]);
  expect(result.current.selectedId).toBeNull();
});

it('loads newest entries first, selects one and requests its messages once', async () => {
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.entries.map(entry => entry.id)).toEqual(['b', 'a']));
  await waitFor(() => expect(result.current.messagesByConversation.b).toEqual([]));
  expect(VaultAPI.listJournalConversations).toHaveBeenCalledWith({ journalSpaceId: 'journal-a', includeArchived: true, limit: 100, offset: 0 });
  expect(result.current.selectedId).toBe('b');
  await act(async () => { expect(await result.current.loadMessages('b')).toEqual([]); });
  expect(VaultAPI.getConversationMessages).toHaveBeenCalledExactlyOnceWith('b');
});

it('preserves persisted pins while the initial list is still loading', async () => {
  localStorage.setItem('journal.pinnedEntries.journal-a', JSON.stringify(['a', 'deleted']));
  const pending = deferred<ListResult>();
  vi.mocked(VaultAPI.listJournalConversations).mockReturnValue(pending.promise);
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await act(async () => { pending.resolve(response([first, second])); });
  await waitFor(() => expect(result.current.entries.map(entry => entry.id)).toEqual(['a', 'b']));
  expect([...result.current.pinnedIds]).toEqual(['a']);
  expect(JSON.parse(localStorage.getItem('journal.pinnedEntries.journal-a')!)).toEqual(['a']);
});

it('ignores a late response from the previous journal', async () => {
  const old = deferred<ListResult>();
  vi.mocked(VaultAPI.listJournalConversations).mockReturnValueOnce(old.promise).mockResolvedValueOnce(response([{ ...second, id: 'new-journal-entry' }]));
  const { result, rerender } = renderHook(useJournalEntries, { initialProps: initial });
  rerender({ ...initial, journalSpaceId: 'journal-b' });
  await waitFor(() => expect(result.current.selectedId).toBe('new-journal-entry'));
  await act(async () => { old.resolve(response([first])); });
  expect(result.current.entries.map(entry => entry.id)).toEqual(['new-journal-entry']);
  expect(result.current.selectedId).toBe('new-journal-entry');
});

it('ignores an older reload failure after a newer reload succeeds', async () => {
  const old = deferred<ListResult>();
  vi.mocked(VaultAPI.listJournalConversations).mockReturnValueOnce(old.promise).mockResolvedValueOnce(response([second]));
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await act(async () => { await result.current.reload(); });
  await act(async () => { old.resolve({ ok: false, error: 'Old request failed' }); });
  expect(result.current.entries).toEqual([second]);
  expect(result.current.loadError).toBeNull();
});

it('applies URL selection once and respects a later manual selection', async () => {
  const { result } = renderHook(useJournalEntries, { initialProps: { ...initial, requestedEntryId: 'a' } });
  await waitFor(() => expect(result.current.selectedId).toBe('a'));
  act(() => result.current.setSelectedId('b'));
  await act(async () => { await result.current.reload(); });
  expect(result.current.selectedId).toBe('b');
});

it('combines trimmed case-insensitive search and pin filters and clears an invalid selection', async () => {
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.isLoading).toBe(false));
  act(() => { result.current.togglePinned('a'); result.current.setFilter('pinned'); result.current.setSearch('  CAFÉ  '); });
  expect(result.current.entries.map(entry => entry.id)).toEqual(['a']);
  expect(result.current.selectedId).toBe('a');
  act(() => result.current.togglePinned('a'));
  expect(result.current.entries).toEqual([]);
  expect(result.current.selectedId).toBeNull();
});

it('removes an entry and its pin while moving selection to a remaining entry', async () => {
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.selectedId).toBe('b'));
  act(() => { result.current.togglePinned('b'); result.current.removeEntry('b'); });
  expect(result.current.entries).toEqual([first]);
  expect(result.current.pinnedIds.has('b')).toBe(false);
  expect(result.current.selectedId).toBe('a');
});

it('keeps loaded entries on a refresh failure and clears the error after recovery', async () => {
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.entries).toHaveLength(2));
  vi.mocked(VaultAPI.listJournalConversations).mockResolvedValueOnce({ ok: false, error: 'Database busy' }).mockResolvedValueOnce(response([first]));
  await act(async () => { await result.current.reload(); });
  expect(result.current.loadError).toBe('Database busy');
  expect(result.current.entries).toHaveLength(2);
  await act(async () => { await result.current.reload(); });
  expect(result.current.loadError).toBeNull();
  expect(result.current.entries).toEqual([first]);
});

it('refreshes after an external vault import', async () => {
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.entries).toHaveLength(2));
  vi.mocked(VaultAPI.listJournalConversations).mockResolvedValue(response([{ ...first, id: 'imported' }]));
  act(() => useVaultImportStore.getState().bump('imported'));
  await waitFor(() => expect(result.current.entries[0]?.id).toBe('imported'));
});

it('keeps pins isolated when switching journals after both lists have loaded', async () => {
  localStorage.setItem('journal.pinnedEntries.journal-a', '["a"]');
  localStorage.setItem('journal.pinnedEntries.journal-b', '["b"]');
  const { result, rerender } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.entries[0]?.id).toBe('a'));
  rerender({ ...initial, journalSpaceId: 'journal-b' });
  await waitFor(() => expect(result.current.entries[0]?.id).toBe('b'));
  expect([...result.current.pinnedIds]).toEqual(['b']);
  expect(localStorage.getItem('journal.pinnedEntries.journal-a')).toBe('["a"]');
  expect(localStorage.getItem('journal.pinnedEntries.journal-b')).toBe('["b"]');
});

it('filters Today by the local calendar and excludes invalid timestamps', async () => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date(2026, 9, 8, 12));
  vi.mocked(VaultAPI.listJournalConversations).mockResolvedValue(response([
    { ...first, updatedAt: new Date(2026, 9, 8, 1).toISOString() },
    { ...second, updatedAt: new Date(2026, 9, 7, 23, 59).toISOString() },
    { ...second, id: 'bad-date', updatedAt: 'invalid' },
  ]));
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.isLoading).toBe(false));
  act(() => result.current.setFilter('today'));
  expect(result.current.entries.map(entry => entry.id)).toEqual(['a']);
});

it('normalizes legacy entry and message fields without rendering invalid values', async () => {
  vi.mocked(VaultAPI.listJournalConversations).mockResolvedValue(response([{ id: 'legacy', title: 42, updated_at: '2026-10-08', message_count: 'invalid' }]));
  vi.mocked(VaultAPI.getConversationMessages).mockResolvedValue({ ok: true, data: { total: 1, messages: [{ role: 'unknown', content: 42, created_at: '2026-10-08', metadata: null }] } } as unknown as Awaited<ReturnType<typeof VaultAPI.getConversationMessages>>);
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.messagesByConversation.legacy).toHaveLength(1));
  expect(result.current.entries[0]).toEqual({ id: 'legacy', title: 'Untitled entry', updatedAt: '2026-10-08', messageCount: 0 });
  expect(result.current.messagesByConversation.legacy[0]).toMatchObject({ id: expect.stringMatching(/^message_/), role: 'assistant', content: '', createdAt: '2026-10-08', metadata: null });
});

it('keeps pin actions usable when local storage is full', async () => {
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Storage full'); });
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.entries).toHaveLength(2));
  act(() => result.current.togglePinned('a'));
  expect(result.current.pinnedIds.has('a')).toBe(true);
});

it.each(['not-json', '{}', '[null,12,""]'])('survives malformed pinned preferences: %s', raw => {
  localStorage.setItem('journal.pinnedEntries.journal-a', raw);
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  expect(result.current.pinnedIds.size).toBe(0);
});
