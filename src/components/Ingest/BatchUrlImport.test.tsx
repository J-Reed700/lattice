import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { VaultAPI } from '@/lib/api';
import { deferred } from '@/tests/deferred';
import type { ApiResult } from '@/types';
import type { BatchJobStatus } from '@/types/api/batch';
import type { UrlPreview } from '@/types/api/web';
import { cancelBatchJob, getBatchJobStatus, startBatchUrlImport } from '@/utils/batchImport';

import { BatchUrlImport } from './BatchUrlImport';

vi.mock('@/lib/api', () => ({ VaultAPI: { fetchUrlPreview: vi.fn() } }));
vi.mock('@/utils/batchImport', () => ({ startBatchUrlImport: vi.fn(), getBatchJobStatus: vi.fn(), cancelBatchJob: vi.fn() }));

const URL_A = 'https://example.org/one';
const URL_B = 'https://example.org/two';
const preview = (url: string) => ({ url, title: `Title ${url}`, siteName: 'Example', readingTimeMinutes: 2, wordCount: 400 } as UrlPreview);
const status = (overrides: Partial<BatchJobStatus> = {}): BatchJobStatus => ({
  id: 'job-1', jobId: 'job-1', jobType: 'url_import', status: 'running', totalItems: 2, completedItems: 0, failedItems: 0,
  total_items: 2, completed_items: 0, failed_items: 0, progress: 0, createdAt: '', completedAt: null, items: [], ...overrides,
});
const completed = () => status({ status: 'completed', completedItems: 2, items: [
  { itemId: 'a', target: URL_A, status: 'completed', documentId: 'doc-a' },
  { itemId: 'b', target: URL_B, status: 'completed', documentId: 'doc-b' },
] });

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  vi.mocked(VaultAPI.fetchUrlPreview).mockImplementation(async url => ({ ok: true, data: preview(url) }));
  vi.mocked(startBatchUrlImport).mockResolvedValue('job-1');
  vi.mocked(getBatchJobStatus).mockResolvedValue(status());
  vi.mocked(cancelBatchJob).mockResolvedValue(2);
});
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); });

function show(onImportComplete = vi.fn()) {
  return { ...render(<TooltipProvider><BatchUrlImport onImportComplete={onImportComplete} /></TooltipProvider>), onImportComplete };
}
async function tick(ms: number) { await act(async () => { await vi.advanceTimersByTimeAsync(ms); }); }
async function paste(text = `${URL_A}\n${URL_B}`) {
  fireEvent.change(screen.getByRole('textbox'), { target: { value: text } });
  await tick(500);
}
async function start() {
  fireEvent.click(screen.getByRole('button', { name: /^Import \d/ }));
  await tick(0);
}

it('deduplicates URLs, adds HTTPS to bare hosts and rejects malformed or unsafe schemes', async () => {
  show();
  await paste(` ${URL_A} \n${URL_A}\nexample.org/two\njavascript:alert(1)\nfile:///secret\nftp://example.org\nnot a url\nexample.org with spaces`);
  expect(VaultAPI.fetchUrlPreview).toHaveBeenCalledTimes(2);
  expect(screen.getByRole('button', { name: 'Import 2 URLs' })).toBeEnabled();
  expect(VaultAPI.fetchUrlPreview).toHaveBeenCalledWith(URL_B);
});

it('cancels a pending parse on unmount before it can start any preview requests', async () => {
  const { unmount } = show();
  fireEvent.change(screen.getByRole('textbox'), { target: { value: URL_A } });
  unmount();
  await tick(1000);
  expect(VaultAPI.fetchUrlPreview).not.toHaveBeenCalled();
});

it('discards an old preview after the input changes', async () => {
  const old = deferred<ApiResult<UrlPreview>>();
  vi.mocked(VaultAPI.fetchUrlPreview).mockReturnValueOnce(old.promise);
  show();
  await paste(URL_A);
  await paste(URL_B);
  await act(async () => { old.resolve({ ok: true, data: preview(URL_A) }); });
  expect(screen.queryByText(`Title ${URL_A}`)).not.toBeInTheDocument();
  expect(screen.getByText(`Title ${URL_B}`)).toBeInTheDocument();
});

it('keeps removed URLs absent when their preview resolves later', async () => {
  const pending = deferred<ApiResult<UrlPreview>>();
  vi.mocked(VaultAPI.fetchUrlPreview).mockReturnValueOnce(pending.promise);
  show();
  await paste(URL_A);
  fireEvent.click(screen.getByRole('button', { name: 'Remove URL' }));
  await act(async () => { pending.resolve({ ok: true, data: preview(URL_A) }); });
  expect(screen.getByText('No URLs yet.')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();
});

it('can import a selected URL even when its preview fails', async () => {
  vi.mocked(VaultAPI.fetchUrlPreview).mockResolvedValue({ ok: false, error: 'Preview unavailable' });
  show();
  await paste(URL_A);
  expect(screen.getByText('Failed — Preview unavailable')).toBeInTheDocument();
  await start();
  expect(startBatchUrlImport).toHaveBeenCalledWith([URL_A], { extractArticle: true });
});

it('respects selection and extraction settings, and prevents editing an active batch', async () => {
  show();
  await paste();
  fireEvent.click(screen.getByRole('button', { name: 'Deselect all' }));
  expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'Select all' }));
  fireEvent.click(screen.getByRole('checkbox', { name: `Include ${URL_B}` }));
  fireEvent.click(screen.getByRole('checkbox', { name: 'Extract article text' }));
  await start();
  expect(startBatchUrlImport).toHaveBeenCalledExactlyOnceWith([URL_A], { extractArticle: false });
  expect(screen.getByRole('textbox')).toBeDisabled();
  expect(screen.getByRole('checkbox', { name: 'Extract article text' })).toBeDisabled();
});

it('matches results by URL regardless of order and calls completion once', async () => {
  vi.mocked(getBatchJobStatus).mockResolvedValue(status({ status: 'completed', completedItems: 1, failedItems: 1, items: [
    { itemId: 'unrelated', target: 'https://other.org', status: 'completed', documentId: 'other' },
    { itemId: 'b', target: URL_B, status: 'failed', errorMessage: 'Forbidden', documentId: null },
    { itemId: 'a', target: URL_A, status: 'completed', documentId: 'doc-a' },
  ] }));
  const { onImportComplete } = show();
  await paste(); await start(); await tick(10000);
  expect(screen.getByText('Imported')).toBeInTheDocument();
  expect(screen.getByText('Failed — Forbidden')).toBeInTheDocument();
  expect(onImportComplete).toHaveBeenCalledExactlyOnceWith({ successful: 1, failed: 1 });
  expect(getBatchJobStatus).toHaveBeenCalledOnce();
});

it('reports a start failure, preserves the selection and allows a retry', async () => {
  vi.mocked(startBatchUrlImport).mockRejectedValueOnce(new Error('Cannot write import history')).mockResolvedValueOnce('job-2');
  vi.mocked(getBatchJobStatus).mockResolvedValue(completed());
  show(); await paste(); await start();
  expect(screen.getByRole('alert')).toHaveTextContent('Cannot write import history');
  await start();
  expect(startBatchUrlImport).toHaveBeenCalledTimes(2);
  expect(getBatchJobStatus).toHaveBeenCalledWith('job-2');
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('reports a polling failure without pretending the import completed', async () => {
  vi.mocked(getBatchJobStatus).mockRejectedValue(new Error('Status temporarily unavailable'));
  const { onImportComplete } = show(); await paste(); await start();
  expect(screen.getByRole('alert')).toHaveTextContent('Status temporarily unavailable');
  expect(onImportComplete).not.toHaveBeenCalled();
});

it('retries a failed status read on the same job without launching a duplicate import', async () => {
  vi.mocked(getBatchJobStatus).mockRejectedValueOnce(new Error('Lost status response')).mockResolvedValueOnce(completed());
  const { onImportComplete } = show(); await paste(); await start();
  expect(screen.getByRole('button', { name: /^Import/ })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'Retry status' }));
  await tick(0);
  expect(startBatchUrlImport).toHaveBeenCalledOnce();
  expect(vi.mocked(getBatchJobStatus).mock.calls).toEqual([['job-1'], ['job-1']]);
  expect(onImportComplete).toHaveBeenCalledExactlyOnceWith({ successful: 2, failed: 0 });
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('prevents a second start or premature cancellation while the job is being created', async () => {
  const pending = deferred<string>();
  vi.mocked(startBatchUrlImport).mockReturnValue(pending.promise);
  show(); await paste(); await start();
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: /^Import/ }));
  expect(startBatchUrlImport).toHaveBeenCalledOnce();
  await act(async () => { pending.resolve('created-job'); });
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
  expect(getBatchJobStatus).toHaveBeenCalledWith('created-job');
});

it('ignores a previous job response after cancelling and starting another job', async () => {
  const old = deferred<BatchJobStatus>();
  vi.mocked(getBatchJobStatus).mockReturnValueOnce(old.promise).mockResolvedValueOnce(completed());
  vi.mocked(startBatchUrlImport).mockResolvedValueOnce('job-1').mockResolvedValueOnce('job-2');
  const { onImportComplete } = show(); await paste(); await start();
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' })); await tick(0);
  await start();
  await act(async () => { old.resolve(status({ status: 'failed', failedItems: 2 })); });
  expect(screen.getAllByText('Imported')).toHaveLength(2);
  expect(onImportComplete).toHaveBeenCalledExactlyOnceWith({ successful: 2, failed: 0 });
  expect(getBatchJobStatus).toHaveBeenNthCalledWith(2, 'job-2');
});

it.each(['cancelled', 'canceled'])('shows %s items as retryable, without leaving a running row', async terminal => {
  vi.mocked(getBatchJobStatus).mockResolvedValue(status({ status: terminal, items: [
    { itemId: 'a', target: URL_A, status: terminal, documentId: null },
    { itemId: 'b', target: URL_B, status: terminal, documentId: null },
  ] }));
  show(); await paste(); await start();
  expect(screen.getAllByText('Failed — Import cancelled')).toHaveLength(2);
  expect(screen.getByRole('button', { name: 'Import 2 URLs' })).toBeEnabled();
  expect(screen.queryByRole('button', { name: 'Cancel' })).not.toBeInTheDocument();
});

it('uses completed counts when an older backend omits per-item details', async () => {
  vi.mocked(getBatchJobStatus).mockResolvedValue(status({ status: 'COMPLETED', completedItems: 1, failedItems: 1, progress: NaN }));
  const { onImportComplete } = show(); await paste(); await start();
  expect(screen.getByText('Imported')).toBeInTheDocument();
  expect(screen.getByText('Failed — Import failed')).toBeInTheDocument();
  expect(onImportComplete).toHaveBeenCalledExactlyOnceWith({ successful: 1, failed: 1 });
});

it('does not start polling when unmounted before the native job ID returns', async () => {
  const pending = deferred<string>();
  vi.mocked(startBatchUrlImport).mockReturnValue(pending.promise);
  const { unmount } = show(); await paste(); await start(); unmount();
  await act(async () => { pending.resolve('background-job'); });
  expect(getBatchJobStatus).not.toHaveBeenCalled();
  expect(cancelBatchJob).not.toHaveBeenCalled();
});

it('cancels the native job and prevents a late poll from reporting success', async () => {
  const pending = deferred<BatchJobStatus>();
  vi.mocked(getBatchJobStatus).mockReturnValue(pending.promise);
  const { onImportComplete } = show(); await paste(); await start();
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  await tick(0);
  expect(cancelBatchJob).toHaveBeenCalledExactlyOnceWith('job-1');
  await act(async () => { pending.resolve(completed()); });
  await tick(10000);
  expect(onImportComplete).not.toHaveBeenCalled();
  expect(getBatchJobStatus).toHaveBeenCalledOnce();
  expect(screen.getByRole('textbox')).toBeEnabled();
});

it('keeps the job visible when native cancellation fails and allows cancelling again', async () => {
  vi.mocked(cancelBatchJob).mockRejectedValueOnce(new Error('Cancellation unavailable')).mockResolvedValueOnce(2);
  show(); await paste(); await start();
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  await tick(0);
  expect(screen.getByRole('alert')).toHaveTextContent('Cancellation unavailable');
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  await tick(0);
  expect(cancelBatchJob).toHaveBeenCalledTimes(2);
});

it('stops renderer polling on unmount while leaving the background job intact', async () => {
  const { unmount, onImportComplete } = show(); await paste(); await start();
  unmount(); await tick(10000);
  expect(getBatchJobStatus).toHaveBeenCalledOnce();
  expect(cancelBatchJob).not.toHaveBeenCalled();
  expect(onImportComplete).not.toHaveBeenCalled();
});

it('does not overwrite completed rows when the remaining work times out', async () => {
  vi.mocked(getBatchJobStatus).mockResolvedValue(status({ completedItems: 1, progress: 50, items: [
    { itemId: 'a', target: URL_A, status: 'completed', documentId: 'doc-a' },
    { itemId: 'b', target: URL_B, status: 'processing', documentId: null },
  ] }));
  const { onImportComplete } = show(); await paste(); await start(); await tick(180800);
  expect(screen.getByText('Imported')).toBeInTheDocument();
  expect(screen.getByText('Failed — Import status timed out')).toBeInTheDocument();
  expect(screen.getByRole('alert')).toHaveTextContent('Import status timed out');
  expect(onImportComplete).not.toHaveBeenCalled();
});
