import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { SearchResult as SearchResultType } from '@/types';

import { SearchResult } from './SearchResult';

const makeResult = (overrides: Partial<SearchResultType> = {}): SearchResultType => ({
  id: 'doc-1',
  documentId: 'doc-1',
  title: 'Field Botany',
  content: 'Ferns grow along shaded creek banks.',
  score: 0.82,
  position: 0,
  vectorScore: 0.79,
  bm25Score: 7.4,
  vectorRank: null,
  bm25Rank: null,
  path: '/Library/Botany/Field Botany.pdf',
  metadata: {},
  ...overrides,
});

describe('SearchResult', () => {
  it('renders a human relevance pill and keeps raw vector/bm25 math only in the title attribute', () => {
    render(<SearchResult result={makeResult()} query="ferns" onOpen={vi.fn()} />);

    expect(screen.getByText('Strong match · 82%')).toBeInTheDocument();
    expect(screen.queryByText(/bm25/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/vec /i)).not.toBeInTheDocument();

    const pill = screen.getByText('Strong match · 82%').closest('[title]');
    expect(pill).toHaveAttribute('title', 'Score 0.82 · vec 0.79 · bm25 7.4');
  });

  it('renders percentage match for moderate scores and opens on click', () => {
    const onOpen = vi.fn();
    const result = makeResult({ score: 0.61, vectorScore: null, bm25Score: null });
    render(<SearchResult result={result} onOpen={onOpen} />);

    expect(screen.getByText('61% match')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button'));
    expect(onOpen).toHaveBeenCalledWith(result);
  });
});
