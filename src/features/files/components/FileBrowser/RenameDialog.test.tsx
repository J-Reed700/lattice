import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';
import type { DocumentMetadata } from '@/types/fileBrowser';

import { RenameDialog } from './RenameDialog';

const h = vi.hoisted(() => ({ rename: vi.fn(), toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock('@/features/files/hooks/useRenameDocument', () => ({ useRenameDocument: () => h.rename }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const atlas = { id: 'd1', fileName: 'Atlas of birds.pdf' } as DocumentMetadata;

const renderDialog = () => {
  const onClose = vi.fn();
  const onSuccess = vi.fn();
  render(<RenameDialog document={atlas} onClose={onClose} onSuccess={onSuccess} />);
  const input = screen.getByLabelText('Document Name');
  const type = (value: string) => fireEvent.change(input, { target: { value } });
  const save = () => fireEvent.click(screen.getByRole('button', { name: 'Save' }));
  return { onClose, onSuccess, input, type, save };
};

describe('RenameDialog', () => {
  beforeEach(() => vi.clearAllMocks());

  it('opens with the current name focused and selected', () => {
    const { input } = renderDialog();
    expect(screen.getByRole('dialog', { name: 'Rename Document' })).toBeInTheDocument();
    expect(input).toHaveValue('Atlas of birds.pdf');
    expect(input).toHaveFocus();
    expect(screen.getByText('Current name: Atlas of birds.pdf')).toBeInTheDocument();
  });

  it.each([
    ['   ', 'Document name cannot be empty'],
    ['x'.repeat(256), 'Document name cannot exceed 255 characters'],
    ['birds/of/prey.pdf', 'Document name cannot contain path separators (/ or \\)'],
    ['birds\\prey.pdf', 'Document name cannot contain path separators (/ or \\)'],
  ])('refuses %j', (value, message) => {
    const { type, save } = renderDialog();
    type(value);
    save();
    expect(screen.getByRole('alert')).toHaveTextContent(message);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(h.rename).not.toHaveBeenCalled();
  });

  it('clears the error on typing', () => {
    const { type, save } = renderDialog();
    type('');
    save();
    expect(screen.getByRole('alert')).toBeInTheDocument();
    type('Atlas');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('closes without a call when the name is unchanged', () => {
    const { onClose, type, save } = renderDialog();
    type('  Atlas of birds.pdf  ');
    save();
    expect(onClose).toHaveBeenCalled();
    expect(h.rename).not.toHaveBeenCalled();
  });

  it('saves the trimmed name, showing progress, then closes', async () => {
    const pending = deferred<{ ok: true; data: { message: string } }>();
    h.rename.mockReturnValueOnce(pending.promise);
    const { onClose, onSuccess, type, save } = renderDialog();
    type('  Field guide to owls.pdf ');
    save();
    expect(h.rename).toHaveBeenCalledWith('d1', 'Field guide to owls.pdf');
    expect(screen.getByRole('button', { name: 'Saving...' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Close dialog' })).toBeDisabled();
    await act(async () => { pending.resolve({ ok: true, data: { message: 'Renamed' } }); await pending.promise; });
    expect(h.toast.success).toHaveBeenCalledWith('Document renamed successfully', { message: 'Renamed' });
    expect(onSuccess).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it('shows a refused rename', async () => {
    h.rename.mockResolvedValueOnce({ ok: false, error: 'Name already taken' });
    const { onClose, type, save } = renderDialog();
    type('Owls.pdf');
    save();
    expect(await screen.findByRole('alert')).toHaveTextContent('Name already taken');
    expect(h.toast.error).toHaveBeenCalledWith('Failed to rename document', { message: 'Name already taken' });
    expect(onClose).not.toHaveBeenCalled();
  });

  it('falls back to a generic message for a refusal without one', async () => {
    h.rename.mockResolvedValueOnce({ ok: false, error: '' });
    const { type, save } = renderDialog();
    type('Owls.pdf');
    save();
    expect(await screen.findByRole('alert')).toHaveTextContent('Failed to rename document');
  });

  it('shows a thrown failure', async () => {
    h.rename.mockRejectedValueOnce(new Error('ipc down'));
    const { type, save } = renderDialog();
    type('Owls.pdf');
    save();
    expect(await screen.findByRole('alert')).toHaveTextContent('ipc down');
    h.rename.mockRejectedValueOnce('weird');
    type('Owls 2.pdf');
    save();
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Unknown error'));
  });

  it('closes from Cancel, the close button, and Escape', () => {
    const { onClose } = renderDialog();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close dialog' }));
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(3);
  });
});
