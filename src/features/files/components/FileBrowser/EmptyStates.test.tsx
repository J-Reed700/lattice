import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CorpusEmptyState, ErrorState, FilterEmptyState } from './EmptyStates';

const h = vi.hoisted(() => ({ store: { searchQuery: '', setSearchQuery: vi.fn() } }));
vi.mock('@/features/files/stores/fileBrowserStore', () => ({
  useFileBrowserStore: (selector: (state: unknown) => unknown) => selector(h.store),
}));

describe('EmptyStates', () => {
  beforeEach(() => {
    h.store = { searchQuery: '', setSearchQuery: vi.fn() };
  });

  it('invites a first folder', () => {
    const onAddFolder = vi.fn();
    render(<CorpusEmptyState onAddFolder={onAddFolder} />);
    expect(screen.getByText('Nothing indexed yet.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Add a folder' }));
    expect(onAddFolder).toHaveBeenCalled();
  });

  it('names the search that matched nothing and clears it', () => {
    h.store.searchQuery = '  saffron ';
    render(<FilterEmptyState />);
    expect(screen.getByText('No files match “saffron”.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    expect(h.store.setSearchQuery).toHaveBeenCalledWith('');
  });

  it('offers no clear action without a search', () => {
    render(<FilterEmptyState />);
    expect(screen.getByText('No files match.')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('shows why the files could not load', () => {
    render(<ErrorState message="database is locked" />);
    expect(screen.getByText('Couldn’t load your files.')).toBeInTheDocument();
    expect(screen.getByText('database is locked')).toBeInTheDocument();
  });
});
