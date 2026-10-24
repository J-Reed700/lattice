import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ModelSearchBar } from './ModelSearchBar';

const h = vi.hoisted(() => ({ catalog: {} as Record<string, any> }));
vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => h.catalog }));

describe('ModelSearchBar', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    h.catalog = { searchQuery: '', setSearchQuery: vi.fn(), setFilters: vi.fn() };
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('searches once typing pauses', () => {
    render(<ModelSearchBar />);
    act(() => { vi.advanceTimersByTime(300); });
    h.catalog.setSearchQuery.mockClear();
    h.catalog.setFilters.mockClear();
    const input = screen.getByRole('textbox', { name: 'Search models' });
    fireEvent.change(input, { target: { value: 'whis' } });
    act(() => { vi.advanceTimersByTime(200); });
    fireEvent.change(input, { target: { value: 'whisper' } });
    act(() => { vi.advanceTimersByTime(299); });
    expect(h.catalog.setSearchQuery).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(1); });
    expect(h.catalog.setSearchQuery).toHaveBeenCalledTimes(1);
    expect(h.catalog.setSearchQuery).toHaveBeenCalledWith('whisper');
    expect(h.catalog.setFilters).toHaveBeenCalledWith({ query_text: 'whisper' });
  });

  it('starts from the current query and clears it', () => {
    h.catalog.searchQuery = 'bge';
    render(<ModelSearchBar />);
    expect(screen.getByRole('textbox', { name: 'Search models' })).toHaveValue('bge');
    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    expect(screen.getByRole('textbox', { name: 'Search models' })).toHaveValue('');
    expect(h.catalog.setSearchQuery).toHaveBeenCalledWith('');
    expect(h.catalog.setFilters).toHaveBeenCalledWith({ query_text: null });
    expect(screen.queryByRole('button', { name: 'Clear search' })).not.toBeInTheDocument();
    act(() => { vi.advanceTimersByTime(300); });
    expect(h.catalog.setFilters).toHaveBeenLastCalledWith({ query_text: null });
  });
});
