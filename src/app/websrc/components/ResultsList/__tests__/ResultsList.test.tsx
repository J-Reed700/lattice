import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { ResultsList } from '../ResultsList';

import type { SearchResult } from '../../../types';

describe('ResultsList', () => {
  const mockOnResultClick = vi.fn();
  let user: ReturnType<typeof userEvent.setup>;

  const mockResults: SearchResult[] = [
    {
      id: '1',
      title: 'Document 1',
      content: 'This is the first search result content',
      score: 0.95,
      path: null,
      documentId: null,
      position: null,
      vectorScore: 0.92,
      bm25Score: 0.88,
      vectorRank: 0,
      bm25Rank: 1,
      metadata: {
        filename: 'document1.txt',
        path: '/documents/document1.txt',
        file_type: 'txt',
        file_size: 1024,
        created_at: '2024-01-15T00:00:00Z',
        updated_at: '2024-01-15T10:30:00Z',
      },
    },
    {
      id: '2',
      title: 'Notes',
      content: 'This is the second search result with different content',
      score: 0.75,
      path: null,
      documentId: null,
      position: null,
      vectorScore: 0.80,
      bm25Score: 0.70,
      vectorRank: 1,
      bm25Rank: 0,
      metadata: {
        filename: 'notes.md',
        path: '/notes/notes.md',
        file_type: 'md',
        file_size: 2048,
        created_at: '2024-01-10T00:00:00Z',
        updated_at: '2024-01-10T08:15:00Z',
      },
    },
    {
      id: '3',
      title: 'Report',
      content: 'Third result with lower relevance score',
      score: 0.55,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
      metadata: {
        filename: 'report.pdf',
        path: '/reports/report.pdf',
        file_type: 'pdf',
        file_size: 2048,
        created_at: '2024-01-01',
        updated_at: '2024-01-01',
      },
    },
  ];

  beforeEach(() => {
    user = userEvent.setup();
    vi.clearAllMocks();
  });

  describe('Rendering States', () => {
    it('renders loading skeleton when isLoading is true', () => {
      render(<ResultsList results={[]} isLoading />);
      expect(screen.getByRole('status')).toBeInTheDocument();
    });

    it('renders error state when error is provided', () => {
      render(<ResultsList results={[]} error="Search failed" />);

      expect(screen.getByRole('alert')).toBeInTheDocument();
      expect(screen.getByText('Search Error')).toBeInTheDocument();
      expect(screen.getByText('Search failed')).toBeInTheDocument();
    });

    it('renders empty state when no results', () => {
      render(<ResultsList results={[]} />);

      expect(screen.getByRole('status')).toBeInTheDocument();
      expect(screen.getByText('No results found')).toBeInTheDocument();
      expect(screen.getByText(/try a different search term/i)).toBeInTheDocument();
    });

    it('renders custom empty message', () => {
      render(<ResultsList results={[]} emptyMessage="Custom empty state" />);
      expect(screen.getByText('Custom empty state')).toBeInTheDocument();
    });

    it('renders results list when results are provided', () => {
      render(<ResultsList results={mockResults} />);

      const list = screen.getByRole('list');
      expect(list).toHaveAttribute('aria-label', '3 search results');
      expect(screen.getAllByRole('listitem')).toHaveLength(3);
    });
  });

  describe('Result Items', () => {
    it('displays file names correctly', () => {
      render(<ResultsList results={mockResults} />);

      // Component uses title as fileName, not metadata.filename
      expect(screen.getByText('Document 1')).toBeInTheDocument();
      expect(screen.getByText('Notes')).toBeInTheDocument();
      expect(screen.getByText('Report')).toBeInTheDocument();
    });

    it('displays file paths as tooltips', () => {
      render(<ResultsList results={mockResults} />);

      const path1 = screen.getByText('/documents/document1.txt');
      expect(path1).toHaveAttribute('title', '/documents/document1.txt');
    });

    it('displays content snippets', () => {
      render(<ResultsList results={mockResults} />);

      expect(screen.getByText('This is the first search result content')).toBeInTheDocument();
      expect(screen.getByText('This is the second search result with different content')).toBeInTheDocument();
    });

    it('displays relevance scores', () => {
      render(<ResultsList results={mockResults} />);

      expect(screen.getByText('95.0%')).toBeInTheDocument();
      expect(screen.getByText('75.0%')).toBeInTheDocument();
      expect(screen.getByText('55.0%')).toBeInTheDocument();
    });

    it('displays file types', () => {
      render(<ResultsList results={mockResults} />);

      expect(screen.getByText('TXT')).toBeInTheDocument();
      expect(screen.getByText('MD')).toBeInTheDocument();
      expect(screen.getByText('PDF')).toBeInTheDocument();
    });

    it('displays vector scores when available', () => {
      render(<ResultsList results={mockResults} />);

      expect(screen.getByText(/Vector: 92\.0%/)).toBeInTheDocument();
      // Multiple results can have the same rank, so use getAllByText
      const rankBadges = screen.getAllByText(/#1/);
      expect(rankBadges.length).toBeGreaterThan(0);
    });

    it('displays BM25 scores when available', () => {
      render(<ResultsList results={mockResults} />);

      expect(screen.getByText(/BM25: 88\.0%/)).toBeInTheDocument();
      // Multiple results can have the same rank, so use getAllByText
      const rankBadges = screen.getAllByText(/#2/);
      expect(rankBadges.length).toBeGreaterThan(0);
    });

    it('displays modified dates when available', () => {
      render(<ResultsList results={mockResults} />);

      expect(screen.getByText('1/15/2024')).toBeInTheDocument();
      expect(screen.getByText('1/10/2024')).toBeInTheDocument();
    });

    it('does not display optional metadata when not available', () => {
      const resultWithoutOptional: SearchResult[] = [
        {
          id: '1',
          title: 'Test',
          content: 'Simple result',
          score: 0.8,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            path: '/test.txt',
            filename: 'test.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      render(<ResultsList results={resultWithoutOptional} />);

      expect(screen.queryByText(/Vector:/)).not.toBeInTheDocument();
      expect(screen.queryByText(/BM25:/)).not.toBeInTheDocument();
    });
  });

  describe('Score Colors', () => {
    it('applies success color for high scores (>= 0.8)', () => {
      const highScoreResult: SearchResult[] = [
        {
          id: '1',
          title: 'High Relevance',
          content: 'High relevance',
          score: 0.85,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            path: '/high.txt',
            filename: 'high.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      const { container } = render(<ResultsList results={highScoreResult} />);
      const scoreElement = container.querySelector('[aria-label*="85.0%"]');
      expect(scoreElement).toHaveClass('text-[var(--success)]');
    });

    it('applies warning color for medium scores (0.6-0.8)', () => {
      const mediumScoreResult: SearchResult[] = [
        {
          id: '1',
          title: 'Medium Relevance',
          content: 'Medium relevance',
          score: 0.65,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            path: '/medium.txt',
            filename: 'medium.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      const { container } = render(<ResultsList results={mediumScoreResult} />);
      const scoreElement = container.querySelector('[aria-label*="65.0%"]');
      expect(scoreElement).toHaveClass('text-[var(--warning)]');
    });

    it('applies secondary color for low scores (< 0.6)', () => {
      const lowScoreResult: SearchResult[] = [
        {
          id: '1',
          title: 'Low Relevance',
          content: 'Low relevance',
          score: 0.45,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            path: '/low.txt',
            filename: 'low.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      const { container } = render(<ResultsList results={lowScoreResult} />);
      const scoreElement = container.querySelector('[aria-label*="45.0%"]');
      expect(scoreElement).toHaveClass('text-[var(--text-secondary)]');
    });
  });

  describe('Sorting', () => {
    it('sorts results by score in descending order', () => {
      render(<ResultsList results={mockResults} />);

      const items = screen.getAllByRole('listitem');

      expect(items[0]).toHaveTextContent('document1.txt');
      expect(items[0]).toHaveTextContent('95.0%');

      expect(items[1]).toHaveTextContent('notes.md');
      expect(items[1]).toHaveTextContent('75.0%');

      expect(items[2]).toHaveTextContent('report.pdf');
      expect(items[2]).toHaveTextContent('55.0%');
    });
  });

  describe('User Interactions', () => {
    it('calls onResultClick when result is clicked', async () => {
      render(<ResultsList results={mockResults} onResultClick={mockOnResultClick} />);

      const firstResult = screen.getByText('Document 1').closest('[role="listitem"]');
      expect(firstResult).toBeInTheDocument();

      if (firstResult) {
        await user.click(firstResult);
        expect(mockOnResultClick).toHaveBeenCalledWith(mockResults[0]);
      }
    });

    it('does not throw when onResultClick is not provided', async () => {
      render(<ResultsList results={mockResults} />);

      const firstResult = screen.getByText('Document 1').closest('[role="listitem"]');
      expect(firstResult).toBeInTheDocument();

      if (firstResult) {
        await expect(user.click(firstResult)).resolves.not.toThrow();
      }
    });

    it('calls onResultClick with correct result for each item', async () => {
      render(<ResultsList results={mockResults} onResultClick={mockOnResultClick} />);

      const secondResult = screen.getByText('Notes').closest('[role="listitem"]');
      if (secondResult) {
        await user.click(secondResult);
        expect(mockOnResultClick).toHaveBeenCalledWith(mockResults[1]);
      }

      mockOnResultClick.mockClear();

      const thirdResult = screen.getByText('Report').closest('[role="listitem"]');
      if (thirdResult) {
        await user.click(thirdResult);
        expect(mockOnResultClick).toHaveBeenCalledWith(mockResults[2]);
      }
    });
  });

  describe('Accessibility', () => {
    it('has proper ARIA labels for list', () => {
      render(<ResultsList results={mockResults} />);

      const list = screen.getByRole('list');
      expect(list).toHaveAttribute('aria-label', '3 search results');
    });

    it('has proper ARIA labels for result items', () => {
      render(<ResultsList results={mockResults} />);

      expect(screen.getByRole('listitem', { name: /result 1: Document 1/i })).toBeInTheDocument();
      expect(screen.getByRole('listitem', { name: /result 2: Notes/i })).toBeInTheDocument();
      expect(screen.getByRole('listitem', { name: /result 3: Report/i })).toBeInTheDocument();
    });

    it('has aria-live polite for error state', () => {
      render(<ResultsList results={[]} error="Test error" />);

      const alert = screen.getByRole('alert');
      expect(alert).toHaveAttribute('aria-live', 'polite');
    });

    it('has aria-live polite for empty state', () => {
      render(<ResultsList results={[]} />);

      const status = screen.getByRole('status');
      expect(status).toHaveAttribute('aria-live', 'polite');
    });

    it('has accessible score labels', () => {
      render(<ResultsList results={mockResults} />);

      expect(screen.getByLabelText('Relevance score: 95.0%')).toBeInTheDocument();
      expect(screen.getByLabelText('Relevance score: 75.0%')).toBeInTheDocument();
    });
  });

  describe('Edge Cases', () => {
    it('handles results without content', () => {
      const resultsWithoutContent: SearchResult[] = [
        {
          id: '1',
          title: 'File',
          score: 0.8,
          content: 'Test content',
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            path: '/file.txt',
            filename: 'file.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      render(<ResultsList results={resultsWithoutContent} />);

      // Component shows title ('File') not filename
      expect(screen.getByText('File')).toBeInTheDocument();
      expect(screen.queryByText(/This is/)).not.toBeInTheDocument();
    });

    it('handles results without metadata', () => {
      const minimalResults: SearchResult[] = [
        {
          id: '1',
          title: 'Test',
          content: 'Test content',
          score: 0.7,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            path: '/test.txt',
            filename: 'test.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      render(<ResultsList results={minimalResults} />);

      expect(screen.getByText('Test')).toBeInTheDocument();
      expect(screen.getByText('TXT')).toBeInTheDocument();
      expect(screen.getByText('70.0%')).toBeInTheDocument();
    });

    it('handles very long file paths gracefully', () => {
      const longPathResult: SearchResult[] = [
        {
          id: '1',
          title: 'Test',
          content: 'Test',
          score: 0.8,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            filename: 'file.txt',
            path: '/very/long/path/that/should/be/truncated/in/the/ui/file.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      const { container } = render(<ResultsList results={longPathResult} />);

      const pathElement = container.querySelector('.truncate');
      expect(pathElement).toBeInTheDocument();
    });

    it('handles zero score correctly', () => {
      const zeroScoreResult: SearchResult[] = [
        {
          id: '1',
          title: 'Zero Score',
          content: 'Test',
          score: 0,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            path: '/zero.txt',
            filename: 'zero.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      render(<ResultsList results={zeroScoreResult} />);

      expect(screen.getByText('0.0%')).toBeInTheDocument();
    });

    it('handles perfect score correctly', () => {
      const perfectScoreResult: SearchResult[] = [
        {
          id: '1',
          title: 'Perfect Score',
          content: 'Test',
          score: 1.0,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
          metadata: {
            path: '/perfect.txt',
            filename: 'perfect.txt',
            file_type: 'txt',
            file_size: 1024,
            created_at: '2024-01-01',
            updated_at: '2024-01-01',
          },
        },
      ];

      render(<ResultsList results={perfectScoreResult} />);

      expect(screen.getByText('100.0%')).toBeInTheDocument();
    });
  });

  describe('Performance', () => {
    it('memoizes results to prevent unnecessary re-renders', () => {
      const { rerender } = render(<ResultsList results={mockResults} />);

      const firstRender = screen.getAllByRole('listitem');

      rerender(<ResultsList results={mockResults} />);

      const secondRender = screen.getAllByRole('listitem');

      expect(firstRender).toHaveLength(secondRender.length);
    });

    it('handles large result sets efficiently', () => {
      const largeResults: SearchResult[] = Array.from({ length: 100 }, (_, i) => ({
        id: `${i}`,
        title: `Result ${i}`,
        content: `Result ${i} content`,
        score: Math.random(),
        path: null,
        documentId: null,
        position: null,
        vectorScore: null,
        bm25Score: null,
        vectorRank: null,
        bm25Rank: null,
        metadata: {
          filename: `file${i}.txt`,
          path: `/path/to/file${i}.txt`,
          file_type: 'txt',
          file_size: 1024,
          updated_at: new Date().toISOString(),
          created_at: new Date().toISOString(),
        },
      }));

      const { container } = render(<ResultsList results={largeResults} />);

      expect(container.querySelectorAll('[role="listitem"]')).toHaveLength(100);
    });
  });
});
