import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SearchInterface } from './SearchInterface';

const useSearchQueryMock = vi.fn();

vi.mock('@/features/search/hooks/useSearchQuery', () => ({
  useSearchQuery: (args: unknown) => useSearchQueryMock(args),
}));

vi.mock('@/features/reading/components/FilePreviewModal', () => ({
  FilePreviewModal: () => null,
}));

function renderSearch(initialEntry = '/search') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[initialEntry]}>
        <SearchInterface />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('SearchInterface', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSearchQueryMock.mockReturnValue({
      data: [],
      isLoading: false,
      isFetching: false,
      error: null,
      refetch: vi.fn(),
    });
  });

  it('mounts LibraryHubNav, focuses the search input, and preserves Hybrid/Semantic/Keyword tabs', () => {
    renderSearch('/search');

    expect(screen.getByRole('navigation', { name: 'Library views' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Search' })).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Search your documents' })).toHaveFocus();
    expect(screen.getByRole('tab', { name: 'Hybrid' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Semantic' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Keyword' })).toBeInTheDocument();
  });

  it('reads ?q= from URL on mount and passes it to useSearchQuery immediately', () => {
    renderSearch('/search?q=photosynthesis');

    expect(screen.getByRole('textbox', { name: 'Search your documents' })).toHaveValue('photosynthesis');
    expect(useSearchQueryMock).toHaveBeenCalledWith(
      expect.objectContaining({ query: 'photosynthesis', mode: 'hybrid' }),
    );
  });
});
