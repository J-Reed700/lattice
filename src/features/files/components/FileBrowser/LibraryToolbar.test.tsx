import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';

import { LibraryToolbar } from './LibraryToolbar';

const renderToolbar = (overrides: Partial<Parameters<typeof LibraryToolbar>[0]> = {}) => {
  const props = {
    isRailOpen: true,
    onToggleRail: vi.fn(),
    searchQuery: '',
    onSearchQueryChange: vi.fn(),
    isContentSearchLoading: false,
    sourceCounts: { all: 1200, local: 1000, web: 200 },
    filterBySource: 'all' as const,
    onFilterBySourceChange: vi.fn(),
    sortField: 'modified' as const,
    sortOrder: 'desc' as const,
    onSortChange: vi.fn(),
    groupByDate: false,
    onToggleGroupByDate: vi.fn(),
    ...overrides,
  };
  const view = render(<TooltipProvider><LibraryToolbar {...props} /></TooltipProvider>);
  return { ...props, container: view.container };
};

describe('LibraryToolbar', () => {
  beforeAll(() => {
    // Radix Select reads pointer capture and scrolls its list into view.
    Element.prototype.hasPointerCapture = () => false;
    Element.prototype.releasePointerCapture = () => {};
    Element.prototype.scrollIntoView = () => {};
  });

  it('toggles the rail with a label for its state', () => {
    const props = renderToolbar();
    fireEvent.click(screen.getByRole('button', { name: /Hide sidebar/ }));
    expect(props.onToggleRail).toHaveBeenCalled();
  });

  it('labels a closed rail', () => {
    renderToolbar({ isRailOpen: false });
    expect(screen.getByRole('button', { name: /Show sidebar/ })).toBeInTheDocument();
  });

  it('searches and clears the search', () => {
    const props = renderToolbar({ searchQuery: 'saffron', isContentSearchLoading: true });
    fireEvent.change(screen.getByRole('textbox', { name: 'Search files' }), { target: { value: 'saffron rice' } });
    expect(props.onSearchQueryChange).toHaveBeenCalledWith('saffron rice');
    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    expect(props.onSearchQueryChange).toHaveBeenLastCalledWith('');
  });

  it('offers no clear button for an empty search', () => {
    renderToolbar({ searchQuery: '   ', isContentSearchLoading: true });
    expect(screen.queryByRole('button', { name: 'Clear search' })).not.toBeInTheDocument();
  });

  it('filters by source with counts', () => {
    const props = renderToolbar({ filterBySource: 'web' });
    expect(screen.getByRole('tab', { name: `Web ${(200).toLocaleString()}` })).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(screen.getByRole('tab', { name: `Local ${(1000).toLocaleString()}` }));
    expect(props.onFilterBySourceChange).toHaveBeenCalledWith('local');
  });

  it('sorts by the chosen field and order', () => {
    const props = renderToolbar();
    const trigger = screen.getByRole('combobox', { name: 'Sort files' });
    expect(trigger).toHaveTextContent('Recently modified');
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: 'mouse' });
    fireEvent.click(screen.getByRole('option', { name: 'Name Z–A' }));
    expect(props.onSortChange).toHaveBeenCalledWith('name', 'desc');
  });

  it('toggles grouping by date', () => {
    const props = renderToolbar({ groupByDate: true });
    const toggle = screen.getByRole('button', { name: 'Group by date' });
    expect(toggle).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(toggle);
    expect(props.onToggleGroupByDate).toHaveBeenCalled();
  });
});
