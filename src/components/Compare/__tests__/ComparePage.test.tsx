import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import type { CompareTableDto } from '@/types/api/compare';

import { ComparePage } from '../ComparePage';

const compareDocuments = vi.fn();
const quickCapture = vi.fn();
const toastSuccess = vi.fn();
const toastError = vi.fn();

vi.mock('@/lib/api', () => {
  const api = {
    compareDocuments: (...args: unknown[]) => compareDocuments(...args),
    quickCapture: (...args: unknown[]) => quickCapture(...args),
  };
  return { VaultAPI: api, default: api };
});

vi.mock('@/stores/toastStore', () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccess(...args),
    error: (...args: unknown[]) => toastError(...args),
  },
}));

vi.mock('@/components/Chat/FilePreviewModal', () => ({
  FilePreviewModal: () => null,
}));

const table: CompareTableDto = {
  columns: ['method'],
  rows: [
    {
      documentId: 'doc_1',
      title: 'trial.pdf',
      filePath: '/vault/trial.pdf',
      cells: [{ value: 'randomised controlled trial', citation: null }],
      error: null,
    },
    {
      documentId: 'doc_2',
      title: 'review.pdf',
      filePath: '/vault/review.pdf',
      cells: [{ value: null, citation: null }],
      error: null,
    },
  ],
  modelName: 'llama-3',
  generatedAt: '2026-09-06T12:00:00.000Z',
};

function renderPage(entry: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <MemoryRouter initialEntries={[entry]}>
          <ComparePage />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

describe('ComparePage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    compareDocuments.mockResolvedValue({ ok: true, data: table });
    quickCapture.mockResolvedValue({
      ok: true,
      data: { noteId: 'note_1', noteTitle: 'Daily Notes · Sep 6', created: false },
    });
  });

  it('asks for two documents before offering the editor', () => {
    renderPage('/compare?ids=doc_1');
    expect(
      screen.getByText('Select two or more documents in the Library to compare them.'),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText('Columns')).not.toBeInTheDocument();
    expect(compareDocuments).not.toHaveBeenCalled();
  });

  it('runs the columns from the URL and renders the table', async () => {
    renderPage('/compare?ids=doc_1,doc_2&columns=method');

    await waitFor(() => expect(screen.getByText('trial.pdf')).toBeInTheDocument());
    expect(compareDocuments).toHaveBeenCalledWith({
      documentIds: ['doc_1', 'doc_2'],
      columns: ['method'],
    });
    expect(screen.getByText('randomised controlled trial')).toBeInTheDocument();
    expect(screen.getByText('not stated')).toBeInTheDocument();
    expect(screen.getByText('2 documents · 1 column')).toBeInTheDocument();
  });

  it('re-running with the same columns asks again instead of doing nothing', async () => {
    const user = userEvent.setup();
    renderPage('/compare?ids=doc_1,doc_2&columns=method');

    await waitFor(() => expect(compareDocuments).toHaveBeenCalledTimes(1));
    await user.click(screen.getByRole('button', { name: 'Compare' }));
    await waitFor(() => expect(compareDocuments).toHaveBeenCalledTimes(2));
  });

  it('saves to the journal and names the page it landed on', async () => {
    const user = userEvent.setup();
    renderPage('/compare?ids=doc_1,doc_2&columns=method');

    await waitFor(() => expect(screen.getByText('trial.pdf')).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Save to journal' }));

    await waitFor(() => expect(quickCapture).toHaveBeenCalledTimes(1));
    expect(quickCapture.mock.calls[0][0]).toContain('| Document | method |');
    expect(toastSuccess).toHaveBeenCalledWith(
      'Comparison saved to "Daily Notes · Sep 6".',
      expect.objectContaining({ action: expect.objectContaining({ label: 'Open' }) }),
    );
  });

  it('reports a failed run and offers to try again', async () => {
    compareDocuments.mockResolvedValue({ ok: false, error: 'No model is loaded.' });
    renderPage('/compare?ids=doc_1,doc_2&columns=method');

    await waitFor(() =>
      expect(screen.getByText(/No model is loaded\./)).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });
});
