import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { SearchInterface } from '@/features/search/components/SearchInterface';
import VaultAPI from '@/lib/api';
import type { ApiResult, SearchResult } from '@/types';

import { deferred } from '../deferred';

vi.mock('@/lib/api', () => ({ default: { searchHybrid: vi.fn(), openFileById: vi.fn(), openFile: vi.fn() } }));
// PDF canvas rendering has separate renderer and browser tests. Keep search,
// debounce, QueryClient, query keys, grouping and result actions real here.
vi.mock('@/features/reading/components/ContentViewer', () => ({ ContentViewer: ({ filePath }: { filePath: string }) => <div aria-label="Opened file">{filePath}</div> }));

let client: QueryClient;
beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  vi.mocked(VaultAPI.searchHybrid).mockResolvedValue({ ok: true, data: [] });
});
afterEach(() => { client.clear(); vi.useRealTimers(); });

function show() { return render(<QueryClientProvider client={client}><SearchInterface /></QueryClientProvider>); }
async function tick(ms = 310) { await act(async () => { await vi.advanceTimersByTimeAsync(ms); }); }
async function search(query: string) {
  fireEvent.change(screen.getByPlaceholderText('Search your documents'), { target: { value: query } });
  await tick();
  // React commits the debounced query before QueryClient schedules its notification.
  await tick(10);
}
const hit = (title: string, overrides: Partial<SearchResult> = {}): SearchResult => ({
  id: title, documentId: title, title, content: 'A matching passage', path: `/${title}`, score: 0.8,
  metadata: {}, highlights: [], position: null, vectorScore: null, bm25Score: null, vectorRank: null, bm25Rank: null, ...overrides,
});

it('debounces rapid edits and does not send empty or whitespace-only queries', async () => {
  show();
  await search('   ');
  expect(VaultAPI.searchHybrid).not.toHaveBeenCalled();
  for (const value of ['c', 'ca', 'café']) {
    fireEvent.change(screen.getByPlaceholderText('Search your documents'), { target: { value } });
    await tick(100);
  }
  expect(VaultAPI.searchHybrid).not.toHaveBeenCalled();
  await tick(210);
  expect(VaultAPI.searchHybrid).toHaveBeenCalledExactlyOnceWith('café', 20, 'hybrid');
  await search('');
  expect(VaultAPI.searchHybrid).toHaveBeenCalledOnce();
});

it('keeps the newest search visible when the old request completes last', async () => {
  const old = deferred<ApiResult<SearchResult[]>>();
  vi.mocked(VaultAPI.searchHybrid).mockReturnValueOnce(old.promise).mockResolvedValueOnce({ ok: true, data: [hit('New result')] });
  show();
  await search('old query');
  await search('new query');
  expect(screen.getByText('New result')).toBeInTheDocument();
  await act(async () => { old.resolve({ ok: true, data: [hit('Stale result')] }); });
  await tick(10);
  expect(screen.queryByText('Stale result')).not.toBeInTheDocument();
  expect(screen.getByText('New result')).toBeInTheDocument();
});

it('isolates each search mode in the cache and reuses fresh results on return', async () => {
  vi.mocked(VaultAPI.searchHybrid).mockImplementation(async (_query, _limit, mode) => ({ ok: true, data: [hit(`${mode} result`)] }));
  show();
  await search('same query');
  expect(screen.getByText('hybrid result')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('tab', { name: 'Keyword' }));
  await tick(10);
  expect(screen.getByText('keyword result')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('tab', { name: 'Hybrid' }));
  await tick(10);
  expect(screen.getByText('hybrid result')).toBeInTheDocument();
  expect(VaultAPI.searchHybrid).toHaveBeenCalledTimes(2);
});

it('surfaces errors and clears them when another query succeeds', async () => {
  vi.mocked(VaultAPI.searchHybrid).mockResolvedValueOnce({ ok: false, error: 'Index not ready' }).mockResolvedValueOnce({ ok: true, data: [] });
  show();
  await search('failed');
  expect(screen.getByText("Couldn't search. Index not ready")).toBeInTheDocument();
  await search('recovered');
  expect(screen.queryByText(/Couldn't search/)).not.toBeInTheDocument();
  expect(screen.getByText('No results for “recovered”.')).toBeInTheDocument();
});

it('groups repeated chunks by document, uses the strongest hit and opens the durable document ID', async () => {
  vi.mocked(VaultAPI.searchHybrid).mockResolvedValue({ ok: true, data: [
    hit('Lower-ranked title', { id: 'chunk-1', documentId: 'document-1', score: 0.2 }),
    hit('Best title', { id: 'chunk-2', documentId: 'document-1', score: 0.9, content: 'Better passage' }),
  ] });
  vi.mocked(VaultAPI.openFileById).mockResolvedValue({ ok: true, data: { action: 'render_internal', contentPath: '/saved/doc.md', fileType: 'text' } });
  show();
  await search('document');
  expect(screen.getByText('1 result')).toBeInTheDocument();
  expect(screen.queryByText('Lower-ranked title')).not.toBeInTheDocument();
  fireEvent.click(screen.getByText('Best title'));
  await tick(10);
  expect(VaultAPI.openFileById).toHaveBeenCalledWith('document-1');
  expect(screen.getByLabelText('Opened file')).toHaveTextContent('/saved/doc.md');
});

it('tries the known path when a result ID is stale and reports a second failure', async () => {
  vi.mocked(VaultAPI.searchHybrid).mockResolvedValue({ ok: true, data: [hit('Moved file')] });
  vi.mocked(VaultAPI.openFileById).mockResolvedValue({ ok: false, error: 'ID missing' });
  vi.mocked(VaultAPI.openFile).mockResolvedValue({ ok: false, error: 'File was deleted' });
  show();
  await search('moved');
  fireEvent.click(screen.getByText('Moved file'));
  await tick(10);
  expect(VaultAPI.openFile).toHaveBeenCalledWith('/Moved file');
  expect(screen.getByText('File was deleted')).toBeInTheDocument();
});
