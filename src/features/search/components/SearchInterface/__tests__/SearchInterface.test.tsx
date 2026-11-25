import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, useNavigate } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SearchInterface } from '../SearchInterface';

const h = vi.hoisted(() => ({
  searchHybrid: vi.fn(),
  searchDocuments: vi.fn(),
  openDocument: vi.fn(),
  openFile: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  VaultAPI: {
    searchHybrid: h.searchHybrid,
    searchDocuments: h.searchDocuments,
    openDocument: h.openDocument,
    openFile: h.openFile,
  },
  default: {
    searchHybrid: h.searchHybrid,
    searchDocuments: h.searchDocuments,
    openDocument: h.openDocument,
    openFile: h.openFile,
  },
}));

vi.mock('@/features/reading/components/FilePreviewModal', () => ({
  FilePreviewModal: () => null,
}));

function renderSearch(initialEntry = '/search', extraControls?: ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[initialEntry]}>
        {extraControls}
        <SearchInterface />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('SearchInterface ?q= sync', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.searchHybrid.mockResolvedValue({ ok: true, data: [] });
    h.searchDocuments.mockResolvedValue({ ok: true, data: [] });
  });

  it('initializes from ?q= and runs the search immediately', async () => {
    renderSearch('/search?q=photosynthesis');

    expect(screen.getByRole('textbox', { name: 'Search your documents' })).toHaveValue('photosynthesis');
    await waitFor(() => {
      expect(h.searchHybrid).toHaveBeenCalled();
    });
    expect(JSON.stringify(h.searchHybrid.mock.calls[0])).toContain('photosynthesis');
    expect(await screen.findByText('No results for “photosynthesis”.')).toBeInTheDocument();
  });

  it('syncs when navigating to a new ?q= while already on /search', async () => {
    function NavControls() {
      const navigate = useNavigate();
      return (
        <button type="button" onClick={() => navigate('/search?q=mitosis')}>
          Run mitosis
        </button>
      );
    }

    renderSearch('/search?q=photosynthesis', <NavControls />);

    await waitFor(() => expect(h.searchHybrid).toHaveBeenCalled());
    expect(await screen.findByText('No results for “photosynthesis”.')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Run mitosis' }));
    expect(screen.getByRole('textbox', { name: 'Search your documents' })).toHaveValue('mitosis');
    await waitFor(() =>
      expect(JSON.stringify(h.searchHybrid.mock.calls.at(-1))).toContain('mitosis'),
    );
    expect(await screen.findByText('No results for “mitosis”.')).toBeInTheDocument();
  });
});
