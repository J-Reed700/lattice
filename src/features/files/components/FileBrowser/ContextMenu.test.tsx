import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';


import VaultAPI from '@/lib/api';
import type { DocumentMetadata } from '@/types/fileBrowser';

import { ContextMenu } from './ContextMenu';

const h = vi.hoisted(() => ({
  navigate: vi.fn(),
  openDocumentFile: vi.fn(),
  showDocumentInFolder: vi.fn(),
  spaces: { data: undefined as unknown[] | undefined, isPending: false },
  memberships: { data: undefined as unknown[] | undefined, isPending: false },
  actions: {
    reindex: { mutateAsync: vi.fn() },
    removeFromIndex: { mutateAsync: vi.fn() },
    setSpaceMembership: { mutateAsync: vi.fn(), isPending: false, variables: undefined as undefined | { spaceId: string } },
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/features/files/hooks/useLibraryDocumentActions', () => ({
  openDocumentFile: h.openDocumentFile,
  showDocumentInFolder: h.showDocumentInFolder,
  useDocumentSpaceMembershipsQuery: () => h.memberships,
  useLibraryDocumentActions: () => h.actions,
}));
vi.mock('@/features/spaces/api/queries', () => ({ useSpacesQuery: () => h.spaces }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const herbal = {
  id: 'd1', fileName: 'herbal.pdf', filePath: '/library/herbal.pdf', fileType: 'pdf', category: 'document',
} as DocumentMetadata;
const article = {
  id: 'w1', fileName: 'Bread history', filePath: 'https://example.org/bread', fileType: 'html', category: 'Web_Article',
} as DocumentMetadata;

const renderMenu = (props: Partial<Parameters<typeof ContextMenu>[0]> = {}) => {
  const all = {
    doc: herbal,
    position: { x: 40, y: 50 },
    onClose: vi.fn(),
    onViewInRecall: vi.fn(),
    onRename: vi.fn(),
    onDelete: vi.fn(),
    ...props,
  };
  render(<ContextMenu {...all} />);
  return all;
};

const item = (name: string) => screen.getByRole('menuitem', { name });

describe('ContextMenu', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.spaces = {
      data: [
        { id: 's2', name: 'Medicine', isArchived: false },
        { id: 's1', name: 'Archive', isArchived: true },
        { id: 's3', name: 'Botany', isArchived: false },
      ],
      isPending: false,
    };
    h.memberships = { data: [{ spaceId: 's3' }], isPending: false };
    h.actions.setSpaceMembership.isPending = false;
    h.actions.setSpaceMembership.variables = undefined;
    h.actions.reindex.mutateAsync.mockResolvedValue(undefined);
    h.actions.removeFromIndex.mutateAsync.mockResolvedValue(undefined);
    h.actions.setSpaceMembership.mutateAsync.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('lists the file actions and spaces, open spaces first', () => {
    renderMenu();
    expect(screen.getByRole('menu', { name: 'File context menu' })).toBeInTheDocument();
    expect(screen.getAllByRole('menuitem').map((el) => el.textContent)).toEqual([
      'View in Lattice', 'Ask about this', 'Open in system viewer', 'Show in folder',
      'Rename', 'Copy path', 'Reindex', 'Remove from index', 'Delete',
    ]);
    expect(screen.getAllByRole('separator')).toHaveLength(2);
    const spaces = screen.getAllByRole('menuitemcheckbox');
    expect(spaces.map((el) => el.textContent)).toEqual(['Botany', 'Medicine', 'Archive (archived)']);
    expect(spaces[0]).toHaveAttribute('aria-checked', 'true');
    expect(spaces[1]).toHaveAttribute('aria-checked', 'false');
  });

  it('offers collection actions when the host supports them', () => {
    const onAddToCollection = vi.fn();
    const onRemoveFromCollection = vi.fn();
    const { onClose } = renderMenu({ onAddToCollection, onRemoveFromCollection });
    fireEvent.click(item('Add to collection…'));
    expect(onAddToCollection).toHaveBeenCalledWith(herbal);
    fireEvent.click(item('Remove from collection'));
    expect(onRemoveFromCollection).toHaveBeenCalledWith(herbal);
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('opens the document inside Lattice', () => {
    const { onViewInRecall, onClose } = renderMenu();
    fireEvent.click(item('View in Lattice'));
    expect(onViewInRecall).toHaveBeenCalledWith(herbal);
    expect(onClose).toHaveBeenCalled();
  });

  it('hides View in Lattice for a file it cannot show', () => {
    renderMenu({ doc: { ...herbal, filePath: '/library/archive.xyz', fileName: 'archive.xyz' } });
    expect(screen.queryByRole('menuitem', { name: 'View in Lattice' })).not.toBeInTheDocument();
  });

  it('starts a new chat about the document', () => {
    renderMenu();
    fireEvent.click(item('Ask about this'));
    expect(h.navigate).toHaveBeenCalledWith('/chat?new=1&documentId=d1');
  });

  describe('open in system viewer', () => {
    it('opens externally and closes', async () => {
      h.openDocumentFile.mockResolvedValueOnce({ ok: true, data: { action: 'opened_external' } });
      const { onClose, onViewInRecall } = renderMenu();
      fireEvent.click(item('Open in system viewer'));
      await waitFor(() => expect(onClose).toHaveBeenCalled());
      expect(h.openDocumentFile).toHaveBeenCalledWith('d1', VaultAPI);
      expect(onViewInRecall).not.toHaveBeenCalled();
    });

    it('falls back to the internal viewer when asked to', async () => {
      h.openDocumentFile.mockResolvedValueOnce({ ok: true, data: { action: 'render_internal' } });
      const { onViewInRecall } = renderMenu();
      fireEvent.click(item('Open in system viewer'));
      await waitFor(() => expect(onViewInRecall).toHaveBeenCalledWith(herbal));
    });

    it('reports a failure to open', async () => {
      h.openDocumentFile.mockResolvedValueOnce({ ok: false, error: 'no app for .pdf' });
      renderMenu();
      fireEvent.click(item('Open in system viewer'));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith('Failed to open file in system viewer', { message: 'no app for .pdf' }));
    });
  });

  it('shows the file in its folder, and reports a failure', async () => {
    const { onClose } = renderMenu();
    h.showDocumentInFolder.mockResolvedValueOnce(undefined);
    fireEvent.click(item('Show in folder'));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    h.showDocumentInFolder.mockRejectedValueOnce(new Error('folder gone'));
    fireEvent.click(item('Show in folder'));
    await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith('Failed to show in folder', { message: 'folder gone' }));
  });

  it('renames through the host', () => {
    const { onRename } = renderMenu();
    fireEvent.click(item('Rename'));
    expect(onRename).toHaveBeenCalledWith(herbal);
  });

  it('disables rename and delete without host handlers', () => {
    renderMenu({ onRename: undefined, onDelete: undefined });
    expect(item('Rename')).toBeDisabled();
    expect(item('Delete')).toBeDisabled();
  });

  it('copies the path, and survives a refused clipboard', async () => {
    const writeText = vi.fn().mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error('denied'));
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { onClose } = renderMenu();
    fireEvent.click(item('Copy path'));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(writeText).toHaveBeenCalledWith('/library/herbal.pdf');
    fireEvent.click(item('Copy path'));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(2));
    expect(error).toHaveBeenCalled();
  });

  it('reindexes and reports either way', async () => {
    renderMenu();
    fireEvent.click(item('Reindex'));
    await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Reindexing herbal.pdf'));
    h.actions.reindex.mutateAsync.mockRejectedValueOnce(new Error('locked'));
    fireEvent.click(item('Reindex'));
    await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't reindex herbal.pdf", { message: 'locked' }));
  });

  it('removes from the index and reports either way', async () => {
    renderMenu();
    fireEvent.click(item('Remove from index'));
    await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Removed herbal.pdf from the index'));
    h.actions.removeFromIndex.mutateAsync.mockRejectedValueOnce('busy');
    fireEvent.click(item('Remove from index'));
    await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't remove herbal.pdf from the index", { message: 'busy' }));
  });

  it('deletes through the host', () => {
    const { onDelete, onClose } = renderMenu();
    fireEvent.click(item('Delete'));
    expect(onDelete).toHaveBeenCalledWith(herbal);
    expect(onClose).toHaveBeenCalled();
  });

  it('limits a web page to the actions that apply', () => {
    renderMenu({ doc: article });
    expect(item('View in Lattice')).toBeEnabled();
    expect(item('Show in folder')).toBeDisabled();
    expect(item('Reindex')).toBeDisabled();
    expect(item('Remove from index')).toBeDisabled();
    fireEvent.click(item('Reindex'));
    expect(h.actions.reindex.mutateAsync).not.toHaveBeenCalled();
  });

  it('treats an archived web copy as a web page', () => {
    renderMenu({ doc: { ...herbal, filePath: '/data/.lattice/web-archive/x.html', category: 'web' } });
    expect(item('Reindex')).toBeDisabled();
  });

  describe('spaces', () => {
    it('toggles the document in or out of a space', async () => {
      renderMenu();
      fireEvent.click(screen.getByRole('menuitemcheckbox', { name: 'Botany' }));
      fireEvent.click(screen.getByRole('menuitemcheckbox', { name: 'Medicine' }));
      await waitFor(() => expect(h.actions.setSpaceMembership.mutateAsync).toHaveBeenCalledTimes(2));
      expect(h.actions.setSpaceMembership.mutateAsync).toHaveBeenNthCalledWith(1, { documentId: 'd1', spaceId: 's3', assigned: false });
      expect(h.actions.setSpaceMembership.mutateAsync).toHaveBeenNthCalledWith(2, { documentId: 'd1', spaceId: 's2', assigned: true });
    });

    it('reports a failed toggle', async () => {
      h.actions.setSpaceMembership.mutateAsync.mockRejectedValueOnce(new Error('space archived'));
      renderMenu();
      fireEvent.click(screen.getByRole('menuitemcheckbox', { name: 'Medicine' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith('Failed to update document scope', { message: 'space archived' }));
    });

    it('disables the space being saved', () => {
      h.actions.setSpaceMembership.isPending = true;
      h.actions.setSpaceMembership.variables = { spaceId: 's2' };
      renderMenu();
      expect(screen.getByRole('menuitemcheckbox', { name: 'Medicine' })).toBeDisabled();
      expect(screen.getByRole('menuitemcheckbox', { name: 'Botany' })).toBeEnabled();
    });

    it('shows loading, then an empty state', () => {
      h.spaces = { data: undefined, isPending: true };
      const first = render(<ContextMenu doc={herbal} position={{ x: 0, y: 0 }} onClose={vi.fn()} />);
      expect(screen.getByText('Loading…')).toBeInTheDocument();
      first.unmount();
      h.spaces = { data: [], isPending: false };
      h.memberships = { data: undefined, isPending: false };
      renderMenu();
      expect(screen.getByText('No spaces yet.')).toBeInTheDocument();
    });

    it('treats a pending membership with no data as unknown', () => {
      h.actions.setSpaceMembership.isPending = true;
      renderMenu();
      for (const box of screen.getAllByRole('menuitemcheckbox')) expect(box).toBeEnabled();
    });
  });

  describe('closing and placement', () => {
    it('closes on Escape and on a click outside, not inside', () => {
      const { onClose } = renderMenu();
      fireEvent.mouseDown(item('Rename'));
      expect(onClose).not.toHaveBeenCalled();
      fireEvent.keyDown(document, { key: 'Enter' });
      expect(onClose).not.toHaveBeenCalled();
      fireEvent.keyDown(document, { key: 'Escape' });
      fireEvent.mouseDown(document.body);
      expect(onClose).toHaveBeenCalledTimes(2);
    });

    it('stays at the click point when it fits', () => {
      renderMenu();
      expect(screen.getByRole('menu')).toHaveStyle({ left: '40px', top: '50px' });
    });

    it('moves back on screen near the right and bottom edges', () => {
      vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 240, height: 400 } as DOMRect);
      renderMenu({ position: { x: window.innerWidth - 10, y: window.innerHeight - 10 } });
      expect(screen.getByRole('menu')).toHaveStyle({
        left: `${window.innerWidth - 248}px`,
        top: `${window.innerHeight - 408}px`,
      });
    });
  });
});
