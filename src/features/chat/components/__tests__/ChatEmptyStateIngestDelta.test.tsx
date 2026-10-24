import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatEmptyStateIngestDelta } from '@/features/chat/components/ChatEmptyStateIngestDelta';

const h = vi.hoisted(() => ({ stats: {} as Record<string, unknown> }));
vi.mock('@/features/chat/hooks/useChatEmptyStateStats', () => ({ useChatEmptyStateStats: () => h.stats }));

const renderDelta = () => render(<MemoryRouter><ChatEmptyStateIngestDelta /></MemoryRouter>);
const stats = (totalDocuments: number, documentsAddedToday = 0, documentsAddedThisWeek = 0) => ({
  data: { totalDocuments, documentsAddedToday, documentsAddedThisWeek }, isLoading: false, isError: false,
});

describe('ChatEmptyStateIngestDelta', () => {
  beforeEach(() => { h.stats = stats(0); });

  it('says nothing while loading, on error, or without data', () => {
    for (const state of [{ isLoading: true }, { isError: true }, { data: undefined }]) {
      h.stats = state;
      const { container, unmount } = renderDelta();
      expect(container).toBeEmptyDOMElement();
      unmount();
    }
  });

  it('invites a first folder to an empty library', () => {
    renderDelta();
    expect(screen.getByText(/Nothing indexed yet\./)).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Add a folder' })).toHaveAttribute('href', '/ingest');
  });

  it('leads with what joined today', () => {
    h.stats = stats(1200, 3, 9);
    renderDelta();
    expect(screen.getByText('3')).toBeInTheDocument();
    expect(screen.getByText(/added today/)).toBeInTheDocument();
    expect(screen.queryByText(/added this week/)).not.toBeInTheDocument();
    expect(screen.getByText((1200).toLocaleString())).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Library' })).toHaveAttribute('href', '/files');
  });

  it('falls back to the week, then to the total alone', () => {
    h.stats = stats(40, 0, 5);
    const { unmount } = renderDelta();
    expect(screen.getByText(/added this week/)).toBeInTheDocument();
    unmount();
    h.stats = stats(1);
    renderDelta();
    expect(screen.queryByText(/added/)).not.toBeInTheDocument();
    expect(screen.getByText(/document ·/)).toBeInTheDocument();
  });
});
