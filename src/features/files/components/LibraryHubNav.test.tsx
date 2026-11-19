import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { LibraryHubNav } from './LibraryHubNav';

describe('LibraryHubNav', () => {
  it('renders lens navigation links without tab or button roles and navigates on click', () => {
    const onNavigate = vi.fn();
    render(<LibraryHubNav activeView="files" onNavigate={onNavigate} />);

    const nav = screen.getByRole('navigation', { name: 'Library views' });
    expect(nav).toBeInTheDocument();
    expect(screen.queryAllByRole('tab')).toHaveLength(0);

    expect(screen.getByRole('link', { name: 'Documents' })).toHaveAttribute('aria-current', 'page');
    expect(screen.getByRole('link', { name: 'Search' })).not.toHaveAttribute('aria-current');

    fireEvent.click(screen.getByRole('link', { name: 'Search' }));
    fireEvent.click(screen.getByRole('link', { name: 'Saved References' }));
    fireEvent.click(screen.getByRole('link', { name: 'Compare' }));
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    expect(onNavigate.mock.calls).toEqual([['/search'], ['/references'], ['/compare'], ['/ingest']]);
  });

  it('triggers /ingest on Mod+I outside editable elements', () => {
    const onNavigate = vi.fn();
    render(
      <div>
        <LibraryHubNav activeView="files" onNavigate={onNavigate} />
        <input aria-label="Search box" />
      </div>,
    );

    const input = screen.getByLabelText('Search box');
    fireEvent.keyDown(input, { key: 'i', ctrlKey: true });
    expect(onNavigate).not.toHaveBeenCalled();

    fireEvent.keyDown(window, { key: 'i', ctrlKey: true });
    expect(onNavigate).toHaveBeenCalledWith('/ingest');
  });

  it('shows Back to Documents when activeView is ingest', () => {
    const onNavigate = vi.fn();
    render(<LibraryHubNav activeView="ingest" onNavigate={onNavigate} />);

    expect(screen.queryByRole('button', { name: 'Import' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Back to Documents' }));
    expect(onNavigate).toHaveBeenCalledWith('/files');
  });
});
