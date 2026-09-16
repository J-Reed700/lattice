import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ImportHistory } from './ImportHistory';

const api = vi.hoisted(() => ({ listAllBatchJobs: vi.fn(), getBatchJobDetails: vi.fn(), retryFailedItems: vi.fn(), deleteBatchJob: vi.fn(), open: vi.fn() }));
vi.mock('@/utils/batchHistory', () => api);
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: api.open }));
const job = { id: 'job', jobType: 'file_import', status: 'completed', totalItems: 2, completedItems: 1, failedItems: 1, createdAt: '2026-09-15T12:00:00Z' };
const items = [
  { itemId: 'good', target: '/Downloads/readable.pdf', status: 'completed' },
  { itemId: 'bad', target: '/Downloads/mpep-2100.pdf', status: 'failed', errorMessage: 'PDF extraction timed out' },
];
const completed = { ...job, completedItems: 2, failedItems: 0, items: items.map(item => ({ ...item, status: 'completed', errorMessage: null })) };
const running = { ...job, status: 'running', failedItems: 0, items: [items[0], { ...items[1], status: 'processing', errorMessage: null }] };
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetAllMocks();
  api.listAllBatchJobs.mockResolvedValue([job]);
  api.getBatchJobDetails.mockResolvedValue({ ...job, items });
});

describe('PDF import recovery', () => {
  it('opens a running retry with the unfinished PDF first and an explicit remaining count', async () => {
    api.listAllBatchJobs.mockResolvedValue([{ ...job, status: 'running', failedItems: 0 }]);
    api.getBatchJobDetails.mockResolvedValue(running);
    render(<ImportHistory />);
    expect(await screen.findByText('1 of 2 imported · 1 remaining')).toBeVisible();
    expect(await screen.findByText('Processing — not ready to search yet')).toBeVisible();
    const pending = screen.getByText('mpep-2100.pdf');
    const completed = screen.getByText('readable.pdf');
    expect(pending.compareDocumentPosition(completed) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Remove' })).not.toBeInTheDocument();
  });
  it('opens failed files automatically with filename, path, reason and next steps', async () => {
    render(<ImportHistory />);
    expect(await screen.findByText('mpep-2100.pdf')).toBeVisible();
    expect(screen.getByText('/Downloads/mpep-2100.pdf')).toBeVisible();
    expect(screen.getByText('Failed — PDF extraction timed out')).toBeVisible();
    expect(screen.getByText('1 imported, 1 failed')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Choose replacement for mpep-2100.pdf' })).toBeEnabled();
  });
  it('preserves the failure if the file picker is cancelled', async () => {
    api.open.mockResolvedValue(null);
    const user = userEvent.setup();
    render(<ImportHistory />);
    await user.click(await screen.findByRole('button', { name: 'Choose replacement for mpep-2100.pdf' }));
    expect(api.retryFailedItems).not.toHaveBeenCalled();
    expect(screen.getByText('Failed — PDF extraction timed out')).toBeVisible();
  });
  it('shows retry-start errors without claiming success or losing the PDF', async () => {
    api.retryFailedItems.mockRejectedValue(new Error('Database is locked'));
    const user = userEvent.setup();
    render(<ImportHistory />);
    await user.click(await screen.findByRole('button', { name: 'Retry mpep-2100.pdf' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not start the retry: Database is locked');
    expect(screen.getByText('Failed — PDF extraction timed out')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Retry mpep-2100.pdf' })).toBeEnabled();
  });
  it('replaces only the selected failed file and renders the saved result', async () => {
    const user = userEvent.setup();
    api.open.mockResolvedValue('/Downloads/corrected.pdf');
    api.retryFailedItems.mockImplementation(async () => {
      api.listAllBatchJobs.mockResolvedValue([{ ...job, completedItems: 2, failedItems: 0 }]);
      api.getBatchJobDetails.mockResolvedValue({ ...completed, items: [items[0], { ...items[1], target: '/Downloads/corrected.pdf', status: 'completed', errorMessage: null }] });
      return 'job';
    });
    render(<ImportHistory />);
    await user.click(await screen.findByRole('button', { name: 'Choose replacement for mpep-2100.pdf' }));
    expect(api.retryFailedItems).toHaveBeenCalledWith('job', 'bad', '/Downloads/corrected.pdf');
    expect(await screen.findByText('2 imported')).toBeVisible();
    expect(screen.getByText('corrected.pdf')).toBeVisible();
    expect(screen.getAllByText('Imported')).toHaveLength(2);
    expect(screen.queryByText(/Failed —/)).not.toBeInTheDocument();
  });
  it('keeps a details error visible and lets the user refresh', async () => {
    const user = userEvent.setup();
    api.getBatchJobDetails.mockRejectedValueOnce(new Error('Database unavailable'));
    render(<ImportHistory />);
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not load files: Database unavailable');
    await user.click(screen.getByRole('button', { name: 'Refresh files' }));
    await waitFor(() => expect(screen.getByText('mpep-2100.pdf')).toBeVisible());
  });
  it('identifies a prior standalone import and keeps a retry inside its original batch', async () => {
    const earlier = { ...job, id: 'earlier', totalItems: 1, completedItems: 1, failedItems: 0, createdAt: '2026-09-15T11:00:00Z', items: [{ ...items[0], target: '/Downloads/mpep-9035-appx-p.pdf' }] };
    api.listAllBatchJobs.mockResolvedValue([job, earlier]);
    api.getBatchJobDetails.mockImplementation(async (id: string) => id === 'earlier' ? earlier : { ...job, items });
    api.retryFailedItems.mockImplementation(async () => {
      api.listAllBatchJobs.mockResolvedValue([completed, earlier]);
      api.getBatchJobDetails.mockImplementation(async (id: string) => id === 'earlier' ? earlier : completed);
      return 'job';
    });
    const user = userEvent.setup();
    render(<ImportHistory />);
    expect(await screen.findByRole('button', { name: /mpep-9035-appx-p.pdf Added/ })).toHaveAttribute('aria-expanded', 'false');
    await user.click(screen.getByRole('button', { name: 'Retry mpep-2100.pdf' }));
    expect(await within(screen.getByRole('region', { name: 'Import job' })).findByText('2 imported')).toBeVisible();
    expect(screen.getByRole('heading', { name: '2 imports' })).toBeVisible();
    expect(screen.getAllByRole('region')).toHaveLength(2);
    expect(api.retryFailedItems).toHaveBeenCalledWith('job', 'bad', undefined);
  });
  it.each(['details', 'list'] as const)('ignores an older %s response arriving after a successful retry', async kind => {
    const user = userEvent.setup();
    render(<ImportHistory />);
    await screen.findByText('Failed — PDF extraction timed out');
    const old = deferred<unknown>();
    const mockedRead = kind === 'details' ? api.getBatchJobDetails : api.listAllBatchJobs;
    mockedRead.mockReturnValueOnce(old.promise);
    fireEvent.focus(window);
    await waitFor(() => expect(mockedRead).toHaveBeenCalledTimes(2));
    api.retryFailedItems.mockImplementation(async () => {
      api.listAllBatchJobs.mockResolvedValue([completed]);
      api.getBatchJobDetails.mockResolvedValue(completed);
      return 'job';
    });
    await user.click(screen.getByRole('button', { name: 'Retry mpep-2100.pdf' }));
    expect(await screen.findByText('2 imported')).toBeVisible();
    await act(async () => { old.resolve(kind === 'details' ? running : [running]); });
    expect(screen.getByText('2 imported')).toBeVisible();
    expect(screen.getAllByText('Imported')).toHaveLength(2);
    expect(screen.queryByText('Processing — not ready to search yet')).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '1 import' })).toBeVisible();
  });
  it('uses the same snapshot for the heading and file rows when the list summary lags', async () => {
    api.listAllBatchJobs.mockResolvedValue([running]);
    api.getBatchJobDetails.mockResolvedValue(completed);
    render(<ImportHistory />);
    expect(await screen.findByText('2 imported')).toBeVisible();
    expect(screen.getAllByText('Imported')).toHaveLength(2);
    expect(screen.queryByText(/remaining/)).not.toBeInTheDocument();
  });
  it('refreshes a completed collapsed import when the window regains focus', async () => {
    api.listAllBatchJobs.mockResolvedValue([completed]);
    api.getBatchJobDetails.mockResolvedValue(completed);
    render(<ImportHistory />);
    const heading = await screen.findByRole('button', { name: /2 files Added/ });
    fireEvent.click(heading);
    await screen.findAllByText('Imported');
    fireEvent.click(heading);
    api.listAllBatchJobs.mockResolvedValue([job]);
    api.getBatchJobDetails.mockResolvedValue({ ...job, items });
    fireEvent.focus(window);
    expect(await screen.findByText('1 imported, 1 failed')).toBeVisible();
    expect(heading).toHaveAttribute('aria-expanded', 'false');
    fireEvent.click(heading);
    expect(await screen.findByText('Failed — PDF extraction timed out')).toBeVisible();
  });
  it('keeps polling a collapsed import through completion', async () => {
    api.listAllBatchJobs.mockResolvedValue([running]);
    api.getBatchJobDetails.mockResolvedValue(running);
    render(<ImportHistory />);
    await screen.findByText('Processing — not ready to search yet');
    fireEvent.click(screen.getByRole('button', { name: /2 files Added/ }));
    api.listAllBatchJobs.mockResolvedValue([completed]);
    api.getBatchJobDetails.mockResolvedValue(completed);
    expect(await screen.findByText('2 imported', {}, { timeout: 4000 })).toBeVisible();
    expect(screen.getByRole('button', { name: /2 files Added/ })).toHaveAttribute('aria-expanded', 'false');
  });
  it('surfaces details failures even for a collapsed single-file import', async () => {
    api.listAllBatchJobs.mockResolvedValue([{ ...job, totalItems: 1, completedItems: 1, failedItems: 0 }]);
    api.getBatchJobDetails.mockRejectedValue(new Error('Connection lost'));
    render(<ImportHistory />);
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not load files: Connection lost');
    expect(screen.getByRole('status')).toHaveTextContent('Status unavailable');
    expect(screen.getByRole('button', { name: 'Refresh files' })).toBeEnabled();
  });
});
