import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';
import type { DocumentMetadata } from '@/types/fileBrowser';

import { useLibraryContentSearch } from './useLibraryContentSearch';

const h = vi.hoisted(() => ({
  readDocumentText: vi.fn(),
  setContentSearchMatches: vi.fn(),
  setContentSearchLoading: vi.fn(),
  clearContentSearch: vi.fn(),
}));
vi.mock('@/features/files/hooks/useLibraryDocumentActions', () => ({ readDocumentText: h.readDocumentText }));
vi.mock('@/features/files/stores/fileBrowserStore', () => ({
  useFileBrowserStore: (selector: (state: unknown) => unknown) => selector({
    setContentSearchMatches: h.setContentSearchMatches,
    setContentSearchLoading: h.setContentSearchLoading,
    clearContentSearch: h.clearContentSearch,
  }),
}));

const doc = (id: string, fileName: string, fileType = 'md', extra: Partial<DocumentMetadata> = {}) =>
  ({ id, fileName, filePath: `/notes/${fileName}`, fileType, category: 'document', modifiedAt: '1', indexedAt: '1', ...extra }) as DocumentMetadata;

const texts: Record<string, string> = {
  '/notes/bread.md': 'Knead the DOUGH for ten minutes',
  '/notes/stew.md': 'Brown the onions first',
  '/notes/yeast.txt': 'Yeast eats sugar and makes dough rise',
};

const matchesOf = (call: unknown[]) => Array.from(call[0] as Iterable<string>).sort();

describe('useLibraryContentSearch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    h.readDocumentText.mockImplementation(async (path: string) =>
      path in texts ? { ok: true, data: texts[path] } : { ok: false, error: 'unreadable' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('clears the content search when the query is empty', () => {
    const { result } = renderHook(() => useLibraryContentSearch([doc('1', 'bread.md')], '  ', null));
    expect(h.clearContentSearch).toHaveBeenCalled();
    expect(result.current.activeFilter).toBeNull();
  });

  it('reports no matches at once when every name already matches or cannot be read', () => {
    renderHook(() => useLibraryContentSearch([doc('1', 'dough.md'), doc('2', 'photo.png', 'png')], 'dough', null));
    expect(h.setContentSearchMatches).toHaveBeenCalledWith([]);
    expect(h.setContentSearchLoading).toHaveBeenCalledWith(false);
    expect(h.readDocumentText).not.toHaveBeenCalled();
  });

  it('reads the text after a pause and reports the documents that mention the query', async () => {
    const docs = [doc('1', 'bread.md'), doc('2', 'stew.md'), doc('3', 'yeast.txt', 'txt'), doc('4', 'missing.md')];
    renderHook(() => useLibraryContentSearch(docs, 'Dough', null));
    expect(h.readDocumentText).not.toHaveBeenCalled();
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(h.setContentSearchLoading).toHaveBeenCalledWith(true);
    expect(h.readDocumentText).toHaveBeenCalledTimes(4);
    expect(matchesOf(h.setContentSearchMatches.mock.calls.at(-1)!)).toEqual(['1', '3']);
    expect(h.setContentSearchLoading).toHaveBeenLastCalledWith(false);
  });

  it('searches only the active type facet', async () => {
    const docs = [doc('1', 'bread.md'), doc('3', 'yeast.txt', 'txt')];
    const { result } = renderHook(() => useLibraryContentSearch(docs, 'dough', ' TXT '));
    expect(result.current.activeFilter).toBe('txt');
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(h.readDocumentText).toHaveBeenCalledTimes(1);
    expect(matchesOf(h.setContentSearchMatches.mock.calls.at(-1)!)).toEqual(['3']);
  });

  it('reads each unchanged document once across queries, and again after it changes', async () => {
    const bread = doc('1', 'bread.md');
    const { rerender } = renderHook(({ docs, query }) => useLibraryContentSearch(docs, query, null), {
      initialProps: { docs: [bread], query: 'dough' },
    });
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    rerender({ docs: [bread], query: 'knead' });
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(h.readDocumentText).toHaveBeenCalledTimes(1);
    expect(matchesOf(h.setContentSearchMatches.mock.calls.at(-1)!)).toEqual(['1']);
    // A cached miss is a miss.
    rerender({ docs: [bread], query: 'onions' });
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(matchesOf(h.setContentSearchMatches.mock.calls.at(-1)!)).toEqual([]);
    rerender({ docs: [{ ...bread, modifiedAt: '2' }], query: 'onions' });
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(h.readDocumentText).toHaveBeenCalledTimes(2);
  });

  it('works through large libraries in batches', async () => {
    const docs = Array.from({ length: 14 }, (_v, i) => doc(String(i), `note-${i}.md`));
    h.readDocumentText.mockResolvedValue({ ok: true, data: 'mentions saffron' });
    renderHook(() => useLibraryContentSearch(docs, 'saffron', null));
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(h.readDocumentText).toHaveBeenCalledTimes(14);
    expect(matchesOf(h.setContentSearchMatches.mock.calls.at(-1)!)).toHaveLength(14);
  });

  it('drops a superseded search instead of reporting its matches', async () => {
    const slow = deferred<{ ok: true; data: string }>();
    h.readDocumentText.mockReturnValueOnce(slow.promise);
    const docs = [doc('1', 'bread.md'), ...Array.from({ length: 6 }, (_v, i) => doc(`x${i}`, `x${i}.md`))];
    const { rerender } = renderHook(({ query }) => useLibraryContentSearch(docs, query, null), { initialProps: { query: 'dough' } });
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    rerender({ query: '' });
    await act(async () => { slow.resolve({ ok: true, data: 'dough' }); await slow.promise; });
    expect(h.setContentSearchMatches).not.toHaveBeenCalled();
    expect(h.clearContentSearch).toHaveBeenCalled();
  });

  it('cancels a pending search before it starts', async () => {
    const { rerender } = renderHook(({ query }) => useLibraryContentSearch([doc('1', 'bread.md')], query, null), {
      initialProps: { query: 'dough' },
    });
    rerender({ query: '' });
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(h.readDocumentText).not.toHaveBeenCalled();
  });

  it('reports no matches when reading throws', async () => {
    h.readDocumentText.mockRejectedValueOnce(new Error('ipc down'));
    renderHook(() => useLibraryContentSearch([doc('1', 'bread.md')], 'dough', null));
    await act(async () => { await vi.advanceTimersByTimeAsync(220); });
    expect(h.setContentSearchMatches).toHaveBeenCalledWith([]);
    expect(h.setContentSearchLoading).toHaveBeenLastCalledWith(false);
  });
});
