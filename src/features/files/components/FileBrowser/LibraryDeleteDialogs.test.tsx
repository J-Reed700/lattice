import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { LibraryDeleteDialogs } from './LibraryDeleteDialogs';

const deletion = (overrides: Record<string, unknown> = {}) => ({
  pendingDeleteDoc: null,
  setPendingDeleteDoc: vi.fn(),
  pendingBulkDelete: null,
  setPendingBulkDelete: vi.fn(),
  bulkDeleteCancelled: { current: false },
  deleteProgress: 0,
  isDeleting: false,
  executeBulkDelete: vi.fn().mockResolvedValue(undefined),
  executeDelete: vi.fn().mockResolvedValue(undefined),
  ...overrides,
});

describe('LibraryDeleteDialogs', () => {
  it('confirms deleting one document, or keeps it', async () => {
    const state = deletion({ pendingDeleteDoc: { id: 'd1', fileName: 'herbal.pdf' } });
    render(<LibraryDeleteDialogs deletion={state as never} />);
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText(/Delete "herbal.pdf"\?/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: /^Delete/ }));
    await waitFor(() => expect(state.executeDelete).toHaveBeenCalled());
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    expect(state.setPendingDeleteDoc).toHaveBeenCalledWith(null);
  });

  it('counts a bulk delete and stops it on cancel', async () => {
    const state = deletion({ pendingBulkDelete: { ids: ['a', 'b', 'c'], count: 3 } });
    render(<LibraryDeleteDialogs deletion={state as never} />);
    expect(screen.getByText(/Delete 3 documents\?/)).toBeInTheDocument();
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: /^Delete/ }));
    await waitFor(() => expect(state.executeBulkDelete).toHaveBeenCalled());
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    expect(state.bulkDeleteCancelled.current).toBe(true);
    expect(state.setPendingBulkDelete).toHaveBeenCalledWith(null);
  });

  it('shows progress while deleting', () => {
    const state = deletion({ pendingBulkDelete: { ids: ['a', 'b'], count: 2 }, isDeleting: true, deleteProgress: 1 });
    render(<LibraryDeleteDialogs deletion={state as never} />);
    expect(screen.getByText('Processed 1 of 2 documents. Cancel stops after the current document finishes.')).toBeInTheDocument();
  });
});
