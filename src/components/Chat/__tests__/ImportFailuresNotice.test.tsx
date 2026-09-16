import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ImportFailuresNotice } from '../ImportFailuresNotice';

const { listBatchJobs, getBatchJobStatus } = vi.hoisted(() => ({ listBatchJobs: vi.fn(), getBatchJobStatus: vi.fn() }));
vi.mock('@/lib/api', () => ({ VaultAPI: { listBatchJobs, getBatchJobStatus } }));

function renderNotice() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><MemoryRouter><ImportFailuresNotice /></MemoryRouter></QueryClientProvider>);
}

describe('ImportFailuresNotice', () => {
  beforeEach(() => { vi.clearAllMocks(); getBatchJobStatus.mockResolvedValue({ ok: true, data: { items: [{ target: '/Downloads/mpep-2100.pdf', status: 'failed' }] } }); });
  it('surfaces an older batch marked completed when a PDF failed', async () => {
    listBatchJobs.mockResolvedValue({ ok: true, data: [{ jobType: 'file_import', status: 'completed', failedItems: 1 }] });
    renderNotice();
    expect(await screen.findByRole('status')).toHaveTextContent('1 file failed to import: mpep-2100.pdf');
    expect(screen.getByRole('link', { name: 'Review import history' })).toHaveAttribute('href', '/ingest?tab=history');
  });
  it('does not imply complete coverage when the history request fails', async () => {
    listBatchJobs.mockResolvedValue({ ok: false, error: 'Database unavailable' });
    renderNotice();
    expect(await screen.findByRole('status')).toHaveTextContent('Source coverage could not be checked');
  });
  it('continues to show incomplete coverage while a retry is running', async () => {
    listBatchJobs.mockResolvedValue({ ok: true, data: [{ jobType: 'file_import', status: 'running', failedItems: 0 }] });
    renderNotice();
    expect(await screen.findByRole('status')).toHaveTextContent('Files are still importing');
  });
});
