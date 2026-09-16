import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { SearchInterface } from '../../components/SearchInterface';

import type { UseSearchQueryParams } from '../../hooks/queries/useSearchQuery';
import type { SearchResult } from '../../types';

const searchState: {
  data: SearchResult[];
  isLoading: boolean;
  error: Error | null;
  lastParams: UseSearchQueryParams | null;
} = {
  data: [],
  isLoading: false,
  error: null,
  lastParams: null,
};

// react-pdf pulls in pdfjs, which needs a canvas DOM this environment lacks.
// The viewer is not part of the search flow under test.
vi.mock('../../components/ContentViewer', () => ({
  ContentViewer: () => null,
}));

vi.mock('@/hooks/queries', () => ({
  useSearchQuery: (params: UseSearchQueryParams) => {
    searchState.lastParams = params;
    return {
      data: params.query.trim().length > 0 ? searchState.data : [],
      isLoading: params.query.trim().length > 0 ? searchState.isLoading : false,
      error: params.query.trim().length > 0 ? searchState.error : null,
    };
  },
}));

const result = (overrides: Partial<SearchResult> & { id: string }): SearchResult => ({
  documentId: overrides.id,
  title: 'RAG survey 2025.pdf',
  path: '/Users/example/Documents/Research/RAG survey 2025.pdf',
  content: 'The survey categorizes retrievers into sparse, dense, and hybrid families.',
  score: 0.92,
  vectorScore: 0.9,
  bm25Score: 12,
  vectorRank: 0,
  bm25Rank: 1,
  metadata: {},
  highlights: [],
  ...overrides,
} as SearchResult);

describe('Search flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    searchState.data = [];
    searchState.isLoading = false;
    searchState.error = null;
    searchState.lastParams = null;
  });

  it('stays quiet before anything is typed', () => {
    render(<SearchInterface />);

    expect(screen.getByRole('heading', { name: 'Search' })).toBeInTheDocument();
    expect(screen.getByPlaceholderText('Search your documents')).toBeInTheDocument();
    expect(screen.queryByText(/results/)).not.toBeInTheDocument();
  });

  it('defaults to hybrid mode and switches modes through the tabs', async () => {
    const user = userEvent.setup();
    render(<SearchInterface />);

    expect(screen.getByRole('tab', { name: 'Hybrid' })).toHaveAttribute('aria-selected', 'true');

    await user.click(screen.getByRole('tab', { name: 'Keyword' }));

    expect(screen.getByRole('tab', { name: 'Keyword' })).toHaveAttribute('aria-selected', 'true');
    await waitFor(() => {
      expect(searchState.lastParams?.mode).toBe('keyword');
    });
  });

  it('renders results as rows with a 0-1 fused score and raw component scores', async () => {
    searchState.data = [result({ id: 'chunk-1' })];

    const user = userEvent.setup();
    render(<SearchInterface />);

    await user.type(screen.getByRole('textbox'), 'retriever');

    await waitFor(
      () => {
        expect(screen.getByText('RAG survey 2025.pdf')).toBeInTheDocument();
      },
      { timeout: 2000 }
    );

    expect(screen.getByText('1 result')).toBeInTheDocument();
    expect(screen.getByText('0.92')).toBeInTheDocument();

    // BM25 is an unbounded relevance score, never a percentage.
    expect(screen.getByText('vec 0.90 · bm25 12.0')).toBeInTheDocument();
    expect(screen.queryByText(/1200/)).not.toBeInTheDocument();
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
  });

  it('reports an empty result set with the query echoed back', async () => {
    searchState.data = [];

    const user = userEvent.setup();
    render(<SearchInterface />);

    await user.type(screen.getByRole('textbox'), 'harissa');

    await waitFor(
      () => {
        expect(screen.getByText('No results for “harissa”.')).toBeInTheDocument();
      },
      { timeout: 2000 }
    );
  });

  it('shows a plain error line when the search fails', async () => {
    searchState.error = new Error('Index is not ready.');

    const user = userEvent.setup();
    render(<SearchInterface />);

    await user.type(screen.getByRole('textbox'), 'retriever');

    await waitFor(
      () => {
        expect(screen.getByText("Couldn't search. Index is not ready.")).toBeInTheDocument();
      },
      { timeout: 2000 }
    );
  });

  it('opens a result through the API when its row is clicked', async () => {
    searchState.data = [result({ id: 'chunk-1' })];
    const { VaultAPI } = await import('../../lib/api');
    const openFileById = vi.fn().mockResolvedValue({
      ok: true,
      data: { action: 'open_external' },
    });
    (VaultAPI as unknown as Record<string, unknown>).openFileById = openFileById;

    const user = userEvent.setup();
    render(<SearchInterface />);

    await user.type(screen.getByRole('textbox'), 'retriever');

    const row = await screen.findByText('RAG survey 2025.pdf', {}, { timeout: 2000 });
    await user.click(row);

    await waitFor(() => {
      expect(openFileById).toHaveBeenCalledWith('chunk-1');
    });
  });
});
