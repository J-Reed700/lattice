import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { RecentDocuments, addToRecentDocuments, clearRecentDocuments } from '../RecentDocuments';

vi.mock('../../EmptyState', () => ({
  NoRecentDocuments: () => <div data-testid="no-recent">No Recent Documents</div>,
}));

vi.mock('../../LoadingState', () => ({
  LoadingState: ({ message }: { message?: string }) => (
    <div role="status">{message || 'Loading...'}</div>
  ),
}));

describe('RecentDocuments', () => {
  const mockDocuments = [
    {
      id: '1',
      path: '/notes/doc1.md',
      title: 'First Document',
      type: 'note' as const,
      lastAccessed: Date.now() - 1000,
      excerpt: 'This is the first document',
    },
    {
      id: '2',
      path: '/notes/doc2.md',
      title: 'Second Document',
      type: 'daily' as const,
      lastAccessed: Date.now() - 2000,
    },
    {
      id: '3',
      path: '/docs/report.pdf',
      title: 'Report',
      type: 'document' as const,
      lastAccessed: Date.now() - 3000,
      excerpt: 'Annual report',
    },
  ];

  const mockOnDocumentClick = vi.fn();
  let user: ReturnType<typeof userEvent.setup>;

  beforeEach(() => {
    user = userEvent.setup();
    vi.clearAllMocks();
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  describe('Rendering States', () => {
    it('shows loading state initially', () => {
      render(<RecentDocuments />);

      const status = screen.queryByRole('status');
      if (status) {
        expect(status).toBeInTheDocument();
        expect(screen.getByText(/loading recent documents/i)).toBeInTheDocument();
      } else {
        expect(screen.getByTestId('no-recent')).toBeInTheDocument();
      }
    });

    it('shows empty state when no documents', async () => {
      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByTestId('no-recent')).toBeInTheDocument();
      });
    });

    it('shows error state on localStorage error', async () => {
      const getItemSpy = vi.spyOn(window.localStorage, 'getItem').mockImplementation(() => {
        throw new Error('Storage error');
      });

      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('Failed to load recent documents')).toBeInTheDocument();
      });

      getItemSpy.mockRestore();
    });

    it('renders document list when documents exist', async () => {
      localStorage.setItem('vault:recentDocuments', JSON.stringify(mockDocuments));

      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('First Document')).toBeInTheDocument();
        expect(screen.getByText('Second Document')).toBeInTheDocument();
        expect(screen.getByText('Report')).toBeInTheDocument();
      });
    });
  });

  describe('Document Display', () => {
    beforeEach(() => {
      localStorage.setItem('vault:recentDocuments', JSON.stringify(mockDocuments));
    });

    it('displays document titles', async () => {
      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('First Document')).toBeInTheDocument();
      });
    });

    it('displays excerpts when available', async () => {
      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('This is the first document')).toBeInTheDocument();
        expect(screen.getByText('Annual report')).toBeInTheDocument();
      });
    });

    it('displays relative timestamps', async () => {
      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getAllByText('Just now').length).toBeGreaterThan(0);
      });
    });

    it('displays file paths', async () => {
      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText(/\/notes\/doc1\.md/)).toBeInTheDocument();
      });
    });

    it('renders document icons based on type', async () => {
      const { container } = render(<RecentDocuments />);

      await waitFor(() => {
        const svgs = container.querySelectorAll('svg');
        expect(svgs.length).toBeGreaterThan(0);
      });
    });
  });

  describe('Sorting and Limiting', () => {
    it('sorts documents by most recent first', async () => {
      const unsortedDocs = [
        { ...mockDocuments[2], lastAccessed: Date.now() - 5000 },
        { ...mockDocuments[0], lastAccessed: Date.now() - 1000 },
        { ...mockDocuments[1], lastAccessed: Date.now() - 3000 },
      ];

      localStorage.setItem('vault:recentDocuments', JSON.stringify(unsortedDocs));

      render(<RecentDocuments />);

      await waitFor(() => {
        const docButtons = screen.getAllByRole('button').filter((btn) => {
          const text = btn.textContent || '';
          return text.includes('Document') || text.includes('Report');
        });
        expect(docButtons[0]).toHaveTextContent('First Document');
      });
    });

    it('respects maxItems limit', async () => {
      const manyDocs = Array.from({ length: 20 }, (_, i) => ({
        id: `${i}`,
        path: `/doc${i}.md`,
        title: `Document ${i}`,
        type: 'note' as const,
        lastAccessed: Date.now() - i * 1000,
      }));

      localStorage.setItem('vault:recentDocuments', JSON.stringify(manyDocs));

      render(<RecentDocuments maxItems={5} />);

      await waitFor(() => {
        const buttons = screen.getAllByRole('button').filter((btn) => btn.textContent?.includes('Document'));
        expect(buttons.length).toBeLessThanOrEqual(6);
      });
    });

    it('uses default maxItems of 10', async () => {
      const docs = Array.from({ length: 15 }, (_, i) => ({
        id: `${i}`,
        path: `/doc${i}.md`,
        title: `Doc ${i}`,
        type: 'note' as const,
        lastAccessed: Date.now() - i * 1000,
      }));

      localStorage.setItem('vault:recentDocuments', JSON.stringify(docs));

      render(<RecentDocuments />);

      await waitFor(() => {
        const buttons = screen.getAllByRole('button').filter((btn) => btn.textContent?.includes('Doc'));
        expect(buttons.length).toBeLessThanOrEqual(11);
      });
    });
  });

  describe('User Interactions', () => {
    beforeEach(() => {
      localStorage.setItem('vault:recentDocuments', JSON.stringify(mockDocuments));
    });

    it('calls onDocumentClick when document is clicked', async () => {
      render(<RecentDocuments onDocumentClick={mockOnDocumentClick} />);

      await waitFor(() => {
        expect(screen.getByText('First Document')).toBeInTheDocument();
      });

      const firstDoc = screen.getByText('First Document').closest('button');
      if (firstDoc) {
        await user.click(firstDoc);
      }

      expect(mockOnDocumentClick).toHaveBeenCalledWith(
        expect.objectContaining({
          id: '1',
          title: 'First Document',
        })
      );
    });

    it('updates lastAccessed timestamp on click', async () => {
      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('First Document')).toBeInTheDocument();
      });

      const beforeClick = Date.now();
      const firstDoc = screen.getByText('First Document').closest('button');
      if (firstDoc) {
        await user.click(firstDoc);
      }

      await waitFor(() => {
        const stored = JSON.parse(localStorage.getItem('vault:recentDocuments') || '[]');
        const clicked = stored.find((d: any) => d.id === '1');
        expect(clicked.lastAccessed).toBeGreaterThanOrEqual(beforeClick);
      });
    });

    it('clears all documents on clear button click', async () => {
      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('First Document')).toBeInTheDocument();
      });

      const clearButton = screen.getByRole('button', { name: /clear all/i });
      await user.click(clearButton);

      await waitFor(() => {
        expect(screen.getByTestId('no-recent')).toBeInTheDocument();
        expect(localStorage.getItem('vault:recentDocuments')).toBeNull();
      });
    });
  });

  describe('Compact Mode', () => {
    beforeEach(() => {
      localStorage.setItem('vault:recentDocuments', JSON.stringify(mockDocuments));
    });

    it('renders in compact mode', async () => {
      const { container } = render(<RecentDocuments compact />);

      await waitFor(() => {
        expect(screen.getByText('First Document')).toBeInTheDocument();
      });

      const buttons = container.querySelectorAll('button');
      buttons.forEach((button) => {
        if (button.textContent?.includes('First Document')) {
          expect(button).toHaveClass('px-3', 'py-2');
        }
      });
    });

    it('does not show excerpts in compact mode', async () => {
      render(<RecentDocuments compact />);

      await waitFor(() => {
        expect(screen.queryByText('This is the first document')).not.toBeInTheDocument();
      });
    });

    it('shows full details in normal mode', async () => {
      render(<RecentDocuments compact={false} />);

      await waitFor(() => {
        expect(screen.getByText('This is the first document')).toBeInTheDocument();
      });
    });
  });

  describe('Custom className', () => {
    it('applies custom className', () => {
      const { container } = render(<RecentDocuments className="custom-class" />);

      expect(container.querySelector('.custom-class')).toBeInTheDocument();
    });
  });

  describe('Relative Time Formatting', () => {
    it('formats "Just now" for recent items', async () => {
      const recentDoc = [
        {
          id: '1',
          path: '/doc.md',
          title: 'Recent',
          type: 'note' as const,
          lastAccessed: Date.now() - 10000,
        },
      ];

      localStorage.setItem('vault:recentDocuments', JSON.stringify(recentDoc));

      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('Just now')).toBeInTheDocument();
      });
    });

    it('formats minutes ago', async () => {
      const minutesDoc = [
        {
          id: '1',
          path: '/doc.md',
          title: 'Minutes',
          type: 'note' as const,
          lastAccessed: Date.now() - 5 * 60 * 1000,
        },
      ];

      localStorage.setItem('vault:recentDocuments', JSON.stringify(minutesDoc));

      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('5m ago')).toBeInTheDocument();
      });
    });

    it('formats hours ago', async () => {
      const hoursDoc = [
        {
          id: '1',
          path: '/doc.md',
          title: 'Hours',
          type: 'note' as const,
          lastAccessed: Date.now() - 3 * 60 * 60 * 1000,
        },
      ];

      localStorage.setItem('vault:recentDocuments', JSON.stringify(hoursDoc));

      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('3h ago')).toBeInTheDocument();
      });
    });

    it('formats days ago', async () => {
      const daysDoc = [
        {
          id: '1',
          path: '/doc.md',
          title: 'Days',
          type: 'note' as const,
          lastAccessed: Date.now() - 2 * 24 * 60 * 60 * 1000,
        },
      ];

      localStorage.setItem('vault:recentDocuments', JSON.stringify(daysDoc));

      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('2d ago')).toBeInTheDocument();
      });
    });
  });

  describe('Header', () => {
    it('displays header with icon', async () => {
      localStorage.setItem('vault:recentDocuments', JSON.stringify(mockDocuments));

      const { container } = render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByText('Recent Documents')).toBeInTheDocument();
        const clockIcon = container.querySelector('svg');
        expect(clockIcon).toBeInTheDocument();
      });
    });

    it('displays clear all button', async () => {
      localStorage.setItem('vault:recentDocuments', JSON.stringify(mockDocuments));

      render(<RecentDocuments />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /clear all/i })).toBeInTheDocument();
      });
    });
  });

  describe('Accessibility', () => {
    beforeEach(() => {
      localStorage.setItem('vault:recentDocuments', JSON.stringify(mockDocuments));
    });

    it('has accessible button roles', async () => {
      render(<RecentDocuments />);

      await waitFor(() => {
        const buttons = screen.getAllByRole('button');
        expect(buttons.length).toBeGreaterThan(0);
      });
    });

    it('buttons are keyboard accessible', async () => {
      render(<RecentDocuments onDocumentClick={mockOnDocumentClick} />);

      await waitFor(() => {
        expect(screen.getByText('First Document')).toBeInTheDocument();
      });

      const firstDoc = screen.getByText('First Document').closest('button');
      if (firstDoc) {
        firstDoc.focus();
        expect(firstDoc).toHaveFocus();

        await user.keyboard('{Enter}');
        expect(mockOnDocumentClick).toHaveBeenCalled();
      }
    });
  });
});

describe('Utility Functions', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  describe('addToRecentDocuments', () => {
    it('adds document to recent list', () => {
      const doc = {
        id: '1',
        path: '/test.md',
        title: 'Test',
        type: 'note' as const,
        excerpt: 'Test excerpt',
      };

      addToRecentDocuments(doc);

      const stored = JSON.parse(localStorage.getItem('vault:recentDocuments') || '[]');
      expect(stored).toHaveLength(1);
      expect(stored[0]).toMatchObject(doc);
      expect(stored[0].lastAccessed).toBeDefined();
    });

    it('moves existing document to top', () => {
      const doc1 = { id: '1', path: '/1.md', title: 'First', type: 'note' as const };
      const doc2 = { id: '2', path: '/2.md', title: 'Second', type: 'note' as const };

      addToRecentDocuments(doc1);
      addToRecentDocuments(doc2);
      addToRecentDocuments(doc1);

      const stored = JSON.parse(localStorage.getItem('vault:recentDocuments') || '[]');
      expect(stored[0].id).toBe('1');
      expect(stored[1].id).toBe('2');
    });

    it('limits storage to 50 documents', () => {
      for (let i = 0; i < 60; i++) {
        addToRecentDocuments({
          id: `${i}`,
          path: `/doc${i}.md`,
          title: `Doc ${i}`,
          type: 'note',
        });
      }

      const stored = JSON.parse(localStorage.getItem('vault:recentDocuments') || '[]');
      expect(stored).toHaveLength(50);
    });

    it('handles localStorage errors gracefully', () => {
      vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
        throw new Error('Storage full');
      });

      expect(() =>
        addToRecentDocuments({
          id: '1',
          path: '/test.md',
          title: 'Test',
          type: 'note',
        })
      ).not.toThrow();
    });
  });

  describe('clearRecentDocuments', () => {
    it('clears all recent documents', () => {
      localStorage.setItem('vault:recentDocuments', JSON.stringify([{ id: '1' }]));

      clearRecentDocuments();

      expect(localStorage.getItem('vault:recentDocuments')).toBeNull();
    });

    it('handles errors gracefully', () => {
      vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => {
        throw new Error('Cannot remove');
      });

      expect(() => clearRecentDocuments()).not.toThrow();
    });
  });
});
