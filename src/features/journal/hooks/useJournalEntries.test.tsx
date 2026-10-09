import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook as renderBareHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';
import { toast } from '@/stores/toastStore';
import { useVaultImportStore } from '@/stores/vaultImportStore';
import { deferred } from '@/tests/deferred';

import { useJournalEntries } from './useJournalEntries';

vi.mock('@/lib/api', () => {
  const api = { listJournalConversations: vi.fn(), getConversationMessages: vi.fn(), listJournalEntryPins: vi.fn(), setJournalEntryPinned: vi.fn() };
  return { default: api, VaultAPI: api };
});
vi.mock('@/stores/toastStore', () => ({ toast: { error: vi.fn() } }));
type ListResult = Awaited<ReturnType<typeof VaultAPI.listJournalConversations>>;
const response = (conversations: unknown): ListResult => ({ ok: true, data: { conversations } } as ListResult);
const first = { id: 'a', title: 'Older café entry', updatedAt: '2026-10-07T12:00:00Z', messageCount: 1 };
const second = { id: 'b', title: 'Latest entry', updatedAt: '2026-10-08T12:00:00Z', messageCount: 2 };
const initial = { journalSpaceId: 'journal-a' as string | null, requestedEntryId: null as string | null };
const withoutJournal: typeof initial = { ...initial, journalSpaceId: null };

const pins: Record<string, string[]> = {};
const renderHook = <P, R>(hook: (_props: P) => R, options: { initialProps: P }) => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return renderBareHook(hook, { ...options, wrapper });
};

beforeEach(() => {
  vi.resetAllMocks(); localStorage.clear();
  for (const key of Object.keys(pins)) delete pins[key];
  vi.mocked(VaultAPI.listJournalEntryPins).mockImplementation(async (journalSpaceId) => ({ ok: true, data: [...(pins[journalSpaceId] ?? [])] }));
  vi.mocked(VaultAPI.setJournalEntryPinned).mockImplementation(async ({ journalSpaceId, conversationId, pinned }) => {
    const rest = (pins[journalSpaceId] ?? []).filter(id => id !== conversationId);
    pins[journalSpaceId] = pinned ? [conversationId, ...rest] : rest;
    return { ok: true, data: undefined };
  });
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

it('lists the journal\'s saved pins first', async () => {
  pins['journal-a'] = ['a'];
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.entries.map(entry => entry.id)).toEqual(['a', 'b']));
  expect([...result.current.pinnedIds]).toEqual(['a']);
  expect(VaultAPI.listJournalEntryPins).toHaveBeenCalledWith('journal-a');
});

it('saves a pin to the journal and keeps it after a remount', async () => {
  const before = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(before.result.current.entries).toHaveLength(2));
  act(() => before.result.current.togglePinned('a'));
  await waitFor(() => expect(before.result.current.pinnedIds.has('a')).toBe(true));
  await waitFor(() => expect(VaultAPI.setJournalEntryPinned).toHaveBeenCalledWith({ journalSpaceId: 'journal-a', conversationId: 'a', pinned: true }));
  before.unmount();
  const after = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect([...after.result.current.pinnedIds]).toEqual(['a']));
});

it('puts a pin back when the backend refuses it', async () => {
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.entries).toHaveLength(2));
  const refusal = deferred<Awaited<ReturnType<typeof VaultAPI.setJournalEntryPinned>>>();
  vi.mocked(VaultAPI.setJournalEntryPinned).mockReturnValueOnce(refusal.promise);
  act(() => result.current.togglePinned('a'));
  await waitFor(() => expect(result.current.pinnedIds.has('a')).toBe(true));
  await act(async () => { refusal.resolve({ ok: false, error: 'Database busy' }); });
  await waitFor(() => expect(result.current.pinnedIds.has('a')).toBe(false));
  expect(toast.error).toHaveBeenCalledWith("Couldn't pin that entry", { message: 'Database busy' });
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
  await waitFor(() => expect(result.current.entries.map(entry => entry.id)).toEqual(['a']));
  expect(result.current.selectedId).toBe('a');
  act(() => result.current.togglePinned('a'));
  await waitFor(() => expect(result.current.entries).toEqual([]));
  expect(result.current.selectedId).toBeNull();
});

it('removes an entry and its pin while moving selection to a remaining entry', async () => {
  const { result } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.selectedId).toBe('b'));
  act(() => result.current.togglePinned('b'));
  await waitFor(() => expect(result.current.pinnedIds.has('b')).toBe(true));
  act(() => result.current.removeEntry('b'));
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
  pins['journal-a'] = ['a'];
  pins['journal-b'] = ['b'];
  const { result, rerender } = renderHook(useJournalEntries, { initialProps: initial });
  await waitFor(() => expect(result.current.entries[0]?.id).toBe('a'));
  rerender({ ...initial, journalSpaceId: 'journal-b' });
  await waitFor(() => expect(result.current.entries[0]?.id).toBe('b'));
  expect([...result.current.pinnedIds]).toEqual(['b']);
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

