import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CustomCollection, DocumentMetadata } from '@/types/fileBrowser';

import { AddToCollectionDialog, CollectionDocumentsDialog, RenameCollectionDialog } from './CollectionDialogs';

const h = vi.hoisted(() => ({
  collections: undefined as unknown[] | undefined,
  actions: { addDocuments: vi.fn(), create: vi.fn(), rename: vi.fn() },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock('@/features/files/hooks/useCustomCollectionsQuery', () => ({
  useCustomCollectionsQuery: () => ({ data: h.collections }),
  useCustomCollectionActions: () => h.actions,
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const collection = (id: string, name: string, documentIds: string[], kind: 'manual' | 'snapshot' = 'manual') =>
  ({ id, name, kind, documentIds, parentId: null, createdAt: '', updatedAt: '' }) as CustomCollection;
const doc = (id: string, fileName: string) =>
  ({ id, fileName, filePath: `/lib/${fileName}`, fileType: 'md', category: 'document', modifiedAt: '2026-01-01', indexedAt: '2026-01-01' }) as DocumentMetadata;

describe('AddToCollectionDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.collections = [
      collection('k2', 'Weather', ['a']),
      collection('k1', 'Volcanoes', ['a', 'b']),
      collection('k3', 'Snapshot: rocks', [], 'snapshot'),
    ];
    h.actions.addDocuments.mockResolvedValue(undefined);
  });

  it('lists manual collections by name with what is already in them', () => {
    render(<AddToCollectionDialog documentIds={['a', 'b']} onClose={vi.fn()} />);
    expect(screen.getByText(/2 documents selected\./)).toBeInTheDocument();
    const list = screen.getByLabelText('Collections');
    const buttons = within(list).getAllByRole('button');
    expect(buttons.map((button) => button.textContent)).toEqual(['Volcanoes' + 'Already added', 'Weather' + '1 already added']);
    expect(buttons[0]).toBeDisabled();
    expect(screen.queryByText('Snapshot: rocks')).not.toBeInTheDocument();
  });

  it('filters collections and says when none match', () => {
    render(<AddToCollectionDialog documentIds={['c']} onClose={vi.fn()} />);
    expect(screen.getByText(/1 document selected\./)).toBeInTheDocument();
    fireEvent.change(screen.getByRole('textbox', { name: 'Find a collection' }), { target: { value: 'weath' } });
    expect(within(screen.getByLabelText('Collections')).getAllByRole('button')).toHaveLength(1);
    fireEvent.change(screen.getByRole('textbox', { name: 'Find a collection' }), { target: { value: 'oceans' } });
    expect(screen.getByText('No collections match.')).toBeInTheDocument();
  });

  it('adds to an existing collection and closes', async () => {
    const onClose = vi.fn();
    render(<AddToCollectionDialog documentIds={['c']} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: /Weather/ }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(h.actions.addDocuments).toHaveBeenCalledWith('k2', ['c']);
    expect(h.toast.success).toHaveBeenCalledWith('Added to Weather');
  });

  it('shows why an add failed and stays open', async () => {
    h.actions.addDocuments.mockRejectedValueOnce(new Error('collection deleted'));
    const onClose = vi.fn();
    render(<AddToCollectionDialog documentIds={['c']} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: /Weather/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('collection deleted');
    expect(onClose).not.toHaveBeenCalled();
  });

  it('invites a first collection when there are none', () => {
    h.collections = undefined;
    render(<AddToCollectionDialog documentIds={['c']} onClose={vi.fn()} />);
    expect(screen.getByText('Create your first collection for these documents.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create and add' })).toBeDisabled();
  });

  it('creates a collection and adds the documents to it', async () => {
    h.actions.create.mockResolvedValueOnce('k9');
    const onClose = vi.fn();
    render(<AddToCollectionDialog documentIds={['c']} onClose={onClose} />);
    fireEvent.change(screen.getByLabelText('New collection'), { target: { value: '  Glaciers  ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create and add' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(h.actions.create).toHaveBeenCalledWith('  Glaciers  ');
    expect(h.actions.addDocuments).toHaveBeenCalledWith('k9', ['c']);
    expect(h.toast.success).toHaveBeenCalledWith('Added to Glaciers');
  });

  it('asks for a unique name, and clears the message on typing', async () => {
    h.actions.create.mockResolvedValueOnce(null);
    render(<AddToCollectionDialog documentIds={['c']} onClose={vi.fn()} />);
    fireEvent.change(screen.getByLabelText('New collection'), { target: { value: 'Weather' } });
    fireEvent.submit(screen.getByLabelText('New collection').closest('form')!);
    expect(await screen.findByRole('alert')).toHaveTextContent('Enter a unique collection name.');
    fireEvent.change(screen.getByLabelText('New collection'), { target: { value: 'Weather 2' } });
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows why a create failed', async () => {
    h.actions.create.mockRejectedValueOnce('disk full');
    render(<AddToCollectionDialog documentIds={['c']} onClose={vi.fn()} />);
    fireEvent.change(screen.getByLabelText('New collection'), { target: { value: 'Glaciers' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create and add' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('disk full');
  });

  it('closes from Cancel and Escape', () => {
    const onClose = vi.fn();
    render(<AddToCollectionDialog documentIds={['c']} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});

describe('CollectionDocumentsDialog', () => {
  const library = [doc('a', 'basalt.md'), doc('b', 'granite.md'), doc('c', 'pumice.md'), doc('d', 'obsidian.md')];
  const rocks = collection('k1', 'Rocks', ['a']);

  beforeEach(() => {
    vi.clearAllMocks();
    h.actions.addDocuments.mockResolvedValue(undefined);
  });

  it('offers only documents not already in the collection', () => {
    render(<CollectionDocumentsDialog collection={rocks} documents={library} onClose={vi.fn()} />);
    expect(screen.getByRole('heading', { name: 'Add documents to Rocks' })).toBeInTheDocument();
    expect(screen.getByText('0 selected · 3 available')).toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: 'Select basalt.md' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Add\s+documents/ })).toBeDisabled();
  });

  it('selects one, selects all shown, and deselects them', () => {
    render(<CollectionDocumentsDialog collection={rocks} documents={library} onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select granite.md' }));
    expect(screen.getByRole('button', { name: 'Add 1 document' })).toBeEnabled();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select granite.md' }));
    expect(screen.getByText('0 selected · 3 available')).toBeInTheDocument();
    fireEvent.change(screen.getByRole('textbox', { name: 'Find documents' }), { target: { value: 'AN' } });
    expect(screen.getByText('0 selected · 2 available')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Select shown' }));
    expect(screen.getByText('2 selected · 2 available')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Deselect shown' }));
    expect(screen.getByText('0 selected · 2 available')).toBeInTheDocument();
  });

  it('adds the chosen documents and closes', async () => {
    const onClose = vi.fn();
    render(<CollectionDocumentsDialog collection={rocks} documents={library} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Select shown' }));
    fireEvent.click(screen.getByRole('button', { name: 'Add 3 documents' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(h.actions.addDocuments).toHaveBeenCalledWith('k1', ['b', 'c', 'd']);
    expect(h.toast.success).toHaveBeenCalledWith('Added 3 documents to Rocks');
  });

  it('names a single added document in the singular', async () => {
    render(<CollectionDocumentsDialog collection={rocks} documents={library} onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select pumice.md' }));
    fireEvent.click(screen.getByRole('button', { name: 'Add 1 document' }));
    await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Added 1 document to Rocks'));
  });

  it('reports a failed add and stays open', async () => {
    h.actions.addDocuments.mockRejectedValueOnce(new Error('locked'));
    const onClose = vi.fn();
    render(<CollectionDocumentsDialog collection={rocks} documents={library} onClose={onClose} />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select pumice.md' }));
    fireEvent.click(screen.getByRole('button', { name: 'Add 1 document' }));
    await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith('Documents could not be added to the collection', { message: 'locked' }));
    expect(onClose).not.toHaveBeenCalled();
  });

  it.each([
    [[], 'Your Library is empty. Import documents to get started.'],
    [[doc('a', 'basalt.md')], 'All Library documents are already in this collection.'],
  ])('explains an empty list', (documents, message) => {
    render(<CollectionDocumentsDialog collection={rocks} documents={documents} onClose={vi.fn()} />);
    expect(screen.getByText(message)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Select shown' })).toBeDisabled();
  });

  it('says when the search matches nothing', () => {
    render(<CollectionDocumentsDialog collection={rocks} documents={library} onClose={vi.fn()} />);
    fireEvent.change(screen.getByRole('textbox', { name: 'Find documents' }), { target: { value: 'marble' } });
    expect(screen.getByText('No documents match your search.')).toBeInTheDocument();
  });

  it('closes from Cancel', () => {
    const onClose = vi.fn();
    render(<CollectionDocumentsDialog collection={rocks} documents={library} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onClose).toHaveBeenCalled();
  });
});

describe('RenameCollectionDialog', () => {
  const rocks = collection('k1', 'Rocks', []);

  beforeEach(() => vi.clearAllMocks());

  it('renames and closes', async () => {
    h.actions.rename.mockResolvedValueOnce(true);
    const onClose = vi.fn();
    render(<RenameCollectionDialog collection={rocks} onClose={onClose} />);
    const input = screen.getByRole('textbox', { name: 'Collection name' });
    expect(input).toHaveValue('Rocks');
    fireEvent.change(input, { target: { value: 'Minerals' } });
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(h.actions.rename).toHaveBeenCalledWith('k1', 'Minerals');
  });

  it('asks for a unique name when the rename is refused', async () => {
    h.actions.rename.mockResolvedValueOnce(false);
    render(<RenameCollectionDialog collection={rocks} onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Enter a unique collection name.');
    fireEvent.change(screen.getByRole('textbox', { name: 'Collection name' }), { target: { value: 'Gems' } });
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows why a rename failed', async () => {
    h.actions.rename.mockRejectedValueOnce(new Error('collection gone'));
    render(<RenameCollectionDialog collection={rocks} onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('collection gone');
    h.actions.rename.mockRejectedValueOnce('offline');
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('offline'));
  });

  it('refuses a blank name and closes from Cancel', () => {
    const onClose = vi.fn();
    render(<RenameCollectionDialog collection={rocks} onClose={onClose} />);
    fireEvent.change(screen.getByRole('textbox', { name: 'Collection name' }), { target: { value: '  ' } });
    expect(screen.getByRole('button', { name: 'Rename' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onClose).toHaveBeenCalled();
  });
});
