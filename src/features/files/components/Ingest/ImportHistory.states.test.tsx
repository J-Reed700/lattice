import type { ReactElement } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ImportHistory } from './ImportHistory';

const api = vi.hoisted(() => ({ listAllBatchJobs: vi.fn(), getBatchJobDetails: vi.fn(), retryFailedItems: vi.fn(), deleteBatchJob: vi.fn(), open: vi.fn() }));
vi.mock('@/utils/batchHistory', () => api);
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: api.open }));
vi.mock('@tauri-apps/api/event', () => ({ listen: () => Promise.resolve(() => {}) }));

const base = { id: 'job', jobType: 'file_import', status: 'completed', totalItems: 2, completedItems: 2, failedItems: 0, createdAt: '2026-09-15T12:00:00Z' };
const item = (itemId: string, target: string, status: string, errorMessage: string | null = null) => ({ itemId, target, status, errorMessage });

const renderHistory = (ui: ReactElement = <ImportHistory />) => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(ui, { wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> });
};

describe('ImportHistory states', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.deleteBatchJob.mockResolvedValue(undefined);
  });

  it('says there are no imports yet', async () => {
    api.listAllBatchJobs.mockResolvedValue([]);
    renderHistory();
    expect(screen.getByText('Loading import history…')).toBeInTheDocument();
    expect(await screen.findByText('No imports yet.')).toBeInTheDocument();
  });

  it.each([
    [{ status: 'running', completedItems: 1, failedItems: 1 }, 'Finishing import…'],
    [{ status: 'cancelled', completedItems: 0 }, 'Cancelled'],
    [{ status: 'failed', completedItems: 0 }, 'Failed'],
    [{ status: 'completed' }, '2 imported'],
  ])('labels a job %j as %s', async (overrides, label) => {
    const job = { ...base, ...overrides };
    api.listAllBatchJobs.mockResolvedValue([job]);
    api.getBatchJobDetails.mockResolvedValue({ ...job, items: [] });
    renderHistory();
    expect(await screen.findByText(label)).toBeInTheDocument();
  });

  it.each([
    ['Scanned page has no selectable text', 'Choose a PDF with selectable text, or run OCR on it first.'],
    ['File not found', 'Choose the file again if it has moved or been renamed.'],
    ['Extraction timed out', 'Retry the file. If it times out again, choose a smaller or re-exported PDF.'],
    [null, 'Retry this file, or choose a corrected copy to replace it.'],
  ])('suggests the next step for %j', async (error, hint) => {
    const job = { ...base, completedItems: 1, failedItems: 1 };
    api.listAllBatchJobs.mockResolvedValue([job]);
    api.getBatchJobDetails.mockResolvedValue({ ...job, items: [item('bad', '/Downloads/scan.pdf', 'failed', error), item('ok', '/Downloads/ok.pdf', 'cancelled')] });
    renderHistory();
    expect(await screen.findByText(hint)).toBeInTheDocument();
    expect(screen.getByText('Cancelled')).toBeInTheDocument();
    if (!error) expect(screen.getByText('Failed — Import failed')).toBeInTheDocument();
  });

  it('names URL imports by address and counts them', async () => {
    const urls = { ...base, jobType: 'url_import', totalItems: 2 };
    const single = { ...base, id: 'one', jobType: 'url_import', totalItems: 1, completedItems: 1 };
    api.listAllBatchJobs.mockResolvedValue([urls, single]);
    api.getBatchJobDetails.mockImplementation(async (id: string) => id === 'one'
      ? { ...single, items: [item('u1', 'https://example.org/bread', 'completed')] }
      : { ...urls, items: [item('u2', 'https://example.org/rye', 'pending')] });
    renderHistory();
    expect(await screen.findByText('2 URLs')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: /https:\/\/example.org\/bread/ })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '2 imports' })).toBeInTheDocument();
  });

  it('retries every failed file and reports the refresh', async () => {
    const job = { ...base, completedItems: 1, failedItems: 1 };
    api.listAllBatchJobs.mockResolvedValue([job]);
    api.getBatchJobDetails.mockResolvedValue({ ...job, items: [item('bad', '/a.pdf', 'failed', 'x')] });
    api.retryFailedItems.mockResolvedValue('job');
    const onRefresh = vi.fn();
    renderHistory(<ImportHistory onRefresh={onRefresh} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Retry all failed' }));
    await waitFor(() => expect(api.retryFailedItems).toHaveBeenCalledWith('job', undefined, undefined));
    await waitFor(() => expect(onRefresh).toHaveBeenCalled());
  });

  it('refuses a replacement it cannot import', async () => {
    const job = { ...base, completedItems: 1, failedItems: 1 };
    api.listAllBatchJobs.mockResolvedValue([job]);
    api.getBatchJobDetails.mockResolvedValue({ ...job, items: [item('bad', '/notes/a.md', 'failed', 'x')] });
    api.open.mockResolvedValue('/notes/a.exe');
    renderHistory();
    fireEvent.click(await screen.findByRole('button', { name: 'Choose replacement for a.md' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not start the retry: Choose a supported document file.');
    expect(api.open).toHaveBeenCalledWith(expect.not.objectContaining({ filters: expect.anything() }));
  });

  it('removes an import, or says why not', async () => {
    api.listAllBatchJobs.mockResolvedValue([base]);
    api.getBatchJobDetails.mockResolvedValue({ ...base, items: [] });
    api.deleteBatchJob.mockRejectedValueOnce(new Error('job locked'));
    const onRefresh = vi.fn();
    renderHistory(<ImportHistory onRefresh={onRefresh} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Remove' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not remove this import: job locked');
    fireEvent.click(screen.getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(onRefresh).toHaveBeenCalled());
  });

  it('clears all history, showing a failure', async () => {
    api.listAllBatchJobs.mockResolvedValue([base]);
    api.getBatchJobDetails.mockResolvedValue({ ...base, items: [] });
    api.deleteBatchJob.mockRejectedValueOnce(new Error('disk full'));
    const onRefresh = vi.fn();
    renderHistory(<ImportHistory onRefresh={onRefresh} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Clear all' }));
    expect(await screen.findByText('Could not clear history: disk full')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Clear all' }));
    await waitFor(() => expect(onRefresh).toHaveBeenCalled());
  });

  it('collapses and expands an import', async () => {
    const job = { ...base, completedItems: 1, failedItems: 1 };
    api.listAllBatchJobs.mockResolvedValue([job]);
    api.getBatchJobDetails.mockResolvedValue({ ...job, items: [item('bad', '/a.pdf', 'failed', 'x')] });
    renderHistory();
    const section = await screen.findByRole('region', { name: 'Import job' });
    const toggle = within(section).getAllByRole('button')[0];
    await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
  });
});
