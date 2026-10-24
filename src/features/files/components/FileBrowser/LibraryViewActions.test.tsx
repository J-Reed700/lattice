import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';

import { LibraryViewActions } from './LibraryViewActions';

const renderActions = (overrides: Partial<Parameters<typeof LibraryViewActions>[0]> = {}) => {
  const props = {
    viewMode: 'list' as const,
    setViewMode: vi.fn(),
    isNeighborhoodOpen: false,
    focusedDocumentId: 'd1' as string | null,
    toggleNeighborhood: vi.fn(),
    handleRefresh: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  render(<LibraryViewActions {...props} />, { wrapper: TooltipProvider });
  return props;
};

describe('LibraryViewActions', () => {
  it('switches views', () => {
    const props = renderActions();
    fireEvent.click(screen.getByRole('button', { name: 'Grid view' }));
    fireEvent.click(screen.getByRole('button', { name: 'Tree view' }));
    fireEvent.click(screen.getByRole('button', { name: 'List view' }));
    expect(vi.mocked(props.setViewMode).mock.calls).toEqual([['grid'], ['tree'], ['list']]);
  });

  it('toggles related documents for a focused document and refreshes', async () => {
    const props = renderActions();
    fireEvent.click(screen.getByRole('button', { name: 'Related' }));
    expect(props.toggleNeighborhood).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(props.handleRefresh).toHaveBeenCalled());
  });

  it('disables related documents with nothing focused', () => {
    renderActions({ focusedDocumentId: null, viewMode: 'tree', isNeighborhoodOpen: true });
    expect(screen.getByRole('button', { name: 'Related' })).toBeDisabled();
  });
});
