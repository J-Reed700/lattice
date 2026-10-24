import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LibraryCollectionDialogs } from './LibraryCollectionDialogs';

const h = vi.hoisted(() => ({ props: {} as Record<string, Record<string, any>> }));
vi.mock('./CollectionDialogs', () => ({
  AddToCollectionDialog: (props: Record<string, any>) => { h.props.add = props; return <button type="button" onClick={props.onClose}>Close add</button>; },
  CollectionDocumentsDialog: (props: Record<string, any>) => { h.props.documents = props; return <button type="button" onClick={props.onClose}>Close documents</button>; },
  RenameCollectionDialog: (props: Record<string, any>) => { h.props.rename = props; return <button type="button" onClick={props.onClose}>Close rename</button>; },
}));

const kitchen = { id: 'k1', name: 'Kitchen' };
const collections = (overrides: Record<string, unknown> = {}) => ({
  collectionSelection: null,
  setCollectionSelection: vi.fn(),
  addDocumentsCollection: undefined,
  setAddDocumentsCollectionId: vi.fn(),
  renameCollection: null,
  setRenameCollection: vi.fn(),
  pendingDeleteCollection: null,
  setPendingDeleteCollection: vi.fn(),
  confirmDeleteCollection: vi.fn().mockResolvedValue(undefined),
  ...overrides,
}) as never;

describe('LibraryCollectionDialogs', () => {
  beforeEach(() => { h.props = {}; });

  it('shows nothing when no dialog is asked for', () => {
    render(<LibraryCollectionDialogs collections={collections()} documents={[]} />);
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens each collection dialog and closes it', () => {
    const state = collections({ collectionSelection: ['d1'], addDocumentsCollection: kitchen, renameCollection: kitchen });
    const documents = [{ id: 'd1' }] as never;
    render(<LibraryCollectionDialogs collections={state} documents={documents} />);
    expect(h.props.add.documentIds).toEqual(['d1']);
    expect(h.props.documents).toMatchObject({ collection: kitchen, documents });
    expect(h.props.rename.collection).toBe(kitchen);
    fireEvent.click(screen.getByRole('button', { name: 'Close add' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close documents' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close rename' }));
    const s = state as unknown as Record<string, ReturnType<typeof vi.fn>>;
    expect(s.setCollectionSelection).toHaveBeenCalledWith(null);
    expect(s.setAddDocumentsCollectionId).toHaveBeenCalledWith(null);
    expect(s.setRenameCollection).toHaveBeenCalledWith(null);
  });

  it('confirms deleting a collection, or keeps it', async () => {
    const state = collections({ pendingDeleteCollection: kitchen });
    render(<LibraryCollectionDialogs collections={state} documents={[]} />);
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText(/Delete “Kitchen” and any collections inside it\?/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: /^Delete collection/ }));
    const s = state as unknown as Record<string, ReturnType<typeof vi.fn>>;
    await waitFor(() => expect(s.confirmDeleteCollection).toHaveBeenCalled());
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    expect(s.setPendingDeleteCollection).toHaveBeenCalledWith(null);
  });
});
