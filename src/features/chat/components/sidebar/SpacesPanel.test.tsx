import { createRef } from 'react';

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';

import { SpacesPanel } from './SpacesPanel';

const h = vi.hoisted(() => ({ store: {} as Record<string, unknown>, reducedMotion: false }));

vi.mock('@/shared/conversations/conversationsStore', () => ({ useConversationsStore: () => h.store }));
vi.mock('framer-motion', async (importOriginal) => ({
  ...(await importOriginal<typeof import('framer-motion')>()),
  useReducedMotion: () => h.reducedMotion,
}));

const spaces = [
  { id: GENERAL_SPACE_ID, name: 'General', isArchived: false, accentColor: null, icon: null, description: null },
  { id: 's-hist', name: 'History', isArchived: false, accentColor: '#3366aa', icon: null, description: 'Ancient empires' },
  { id: 's-cook', name: 'Cooking', isArchived: false, accentColor: null, icon: '🍳', description: null },
  { id: 's-old', name: 'Old chemistry', isArchived: true, accentColor: null, icon: '⚗️', description: null },
  { id: 's-old2', name: 'Old botany', isArchived: true, accentColor: null, icon: null, description: null },
];

const makeEditor = (overrides: Record<string, unknown> = {}) => ({
  isSpaceEditorOpen: false,
  setIsSpaceEditorOpen: vi.fn(),
  isSavingSpace: false,
  isArchivingSpace: false,
  isRestoringSpace: false,
  saveSpaceEnvironment: vi.fn().mockResolvedValue(undefined),
  setSelectedSpaceArchived: vi.fn().mockResolvedValue(undefined),
  restoreArchivedSpace: vi.fn().mockResolvedValue(undefined),
  toggleSpaceDeepResearchDefault: vi.fn(),
  spaceNameDraft: 'History',
  setSpaceNameDraft: vi.fn(),
  spaceDescriptionDraft: 'Ancient empires',
  setSpaceDescriptionDraft: vi.fn(),
  spaceIconDraft: '',
  setSpaceIconDraft: vi.fn(),
  spaceAccentDraft: '#3366aa',
  setSpaceAccentDraft: vi.fn(),
  spacePromptDraft: '',
  setSpacePromptDraft: vi.fn(),
  spaceKbDefault: true,
  setSpaceKbDefault: vi.fn(),
  spaceWebDefault: false,
  setSpaceWebDefault: vi.fn(),
  spaceDeepResearchDefault: false,
  ...overrides,
});

const makeNewSpace = (overrides: Record<string, unknown> = {}) => ({
  isCreateSpaceOpen: false,
  setIsCreateSpaceOpen: vi.fn(),
  newSpaceKindDraft: 'standard',
  setNewSpaceKindDraft: vi.fn(),
  newSpaceNameDraft: '',
  setNewSpaceNameDraft: vi.fn(),
  isCreatingSpace: false,
  createSpace: vi.fn().mockResolvedValue(undefined),
  ...overrides,
});

const renderPanel = (opts: { editor?: Record<string, unknown>; newSpace?: Record<string, unknown>; withAnchor?: boolean } = {}) => {
  const anchorRef = createRef<HTMLDivElement>();
  if (opts.withAnchor !== false) {
    const anchor = document.createElement('div');
    anchor.getBoundingClientRect = () => ({ left: 0, right: 240, top: 10, bottom: 610, width: 240, height: 600, x: 0, y: 10, toJSON: () => ({}) });
    (anchorRef as { current: HTMLDivElement | null }).current = anchor;
  }
  const editor = makeEditor(opts.editor);
  const newSpace = makeNewSpace(opts.newSpace);
  const onClose = vi.fn();
  render(<SpacesPanel anchorRef={anchorRef} onClose={onClose} editor={editor as never} newSpace={newSpace as never} />);
  return { editor, newSpace, onClose };
};

/** Run a setter's functional update against a value. */
const applyUpdate = (setter: ReturnType<typeof vi.fn>, previous: boolean) => (setter.mock.calls[0][0] as (v: boolean) => boolean)(previous);

describe('SpacesPanel', () => {
  beforeEach(() => {
    h.reducedMotion = false;
    h.store = {
      spaces,
      selectedSpaceId: null,
      setSelectedSpace: vi.fn(),
      loadConversations: vi.fn().mockResolvedValue(undefined),
    };
  });

  it('lists live spaces and places itself beside the sidebar', () => {
    renderPanel();
    const dialog = screen.getByRole('dialog', { name: 'Spaces' });
    expect(within(dialog).getByText('All spaces')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: /History/ })).toHaveAttribute('title', 'Ancient empires');
    expect(within(dialog).getByRole('button', { name: /Cooking/ })).toHaveAttribute('title', 'Cooking');
    expect(within(dialog).getByText('🍳')).toBeInTheDocument();
    expect(within(dialog).queryByText('Old chemistry')).not.toBeInTheDocument();
    expect(dialog).toHaveStyle({ left: '240px', top: '10px', height: '600px' });
    // Edit needs a selected space.
    expect(within(dialog).getByRole('button', { name: 'Edit space' })).toBeDisabled();
  });

  it('follows the sidebar when the window resizes', () => {
    const original = window.innerWidth;
    renderPanel();
    const dialog = screen.getByRole('dialog', { name: 'Spaces' });
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 400 });
    fireEvent(window, new Event('resize'));
    // 400 - 240 - 12 = 148, floored to the 220 minimum.
    expect(dialog).toHaveStyle({ width: '220px' });
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: original });
  });

  it('renders without an anchor and with reduced motion', () => {
    h.reducedMotion = true;
    renderPanel({ withAnchor: false });
    expect(screen.getByRole('dialog', { name: 'Spaces' })).toBeInTheDocument();
  });

  it('switches space and reloads its conversations', async () => {
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: /History/ }));
    await waitFor(() => expect(h.store.loadConversations).toHaveBeenCalledWith({ spaceId: 's-hist' }));
    expect(h.store.setSelectedSpace).toHaveBeenCalledWith('s-hist');
    fireEvent.click(screen.getByRole('button', { name: /All spaces/ }));
    await waitFor(() => expect(h.store.loadConversations).toHaveBeenCalledWith({ spaceId: null }));
  });

  it('marks the selected space, with or without an accent', () => {
    h.store = { ...h.store, selectedSpaceId: 's-cook' };
    renderPanel();
    expect(screen.getByRole('button', { name: 'Edit space' })).toBeEnabled();
  });

  it('closes from the close button', () => {
    const { onClose } = renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'Close spaces panel' }));
    expect(onClose).toHaveBeenCalled();
  });

  it('closes on Escape', () => {
    const { onClose } = renderPanel();
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  describe('new space form', () => {
    it('toggles open from the New space button', () => {
      const { newSpace } = renderPanel();
      fireEvent.click(screen.getByRole('button', { name: 'New space' }));
      expect(applyUpdate(newSpace.setIsCreateSpaceOpen, false)).toBe(true);
    });

    it('edits the name and kind, and creates', async () => {
      const { newSpace } = renderPanel({ newSpace: { isCreateSpaceOpen: true } });
      const input = screen.getByPlaceholderText(/Space name/);
      fireEvent.change(input, { target: { value: 'Geology' } });
      expect(newSpace.setNewSpaceNameDraft).toHaveBeenCalledWith('Geology');
      fireEvent.click(screen.getByRole('button', { name: 'Journal' }));
      expect(newSpace.setNewSpaceKindDraft).toHaveBeenCalledWith('journal');
      fireEvent.click(screen.getByRole('button', { name: 'Standard' }));
      expect(newSpace.setNewSpaceKindDraft).toHaveBeenCalledWith('standard');
      fireEvent.click(screen.getByRole('button', { name: 'Create' }));
      await waitFor(() => expect(newSpace.createSpace).toHaveBeenCalled());
    });

    it('asks for a journal name when making a journal', () => {
      renderPanel({ newSpace: { isCreateSpaceOpen: true, newSpaceKindDraft: 'journal' } });
      expect(screen.getByPlaceholderText(/Journal name/)).toBeInTheDocument();
    });

    it('cancels and resets the form', () => {
      const { newSpace } = renderPanel({ newSpace: { isCreateSpaceOpen: true } });
      const cancels = screen.getAllByRole('button', { name: 'Cancel' });
      // The header toggle reads Cancel too; the form's own Cancel resets the drafts.
      fireEvent.click(cancels[cancels.length - 1]);
      expect(newSpace.setIsCreateSpaceOpen).toHaveBeenCalledWith(false);
      expect(newSpace.setNewSpaceNameDraft).toHaveBeenCalledWith('');
      expect(newSpace.setNewSpaceKindDraft).toHaveBeenCalledWith('standard');
    });

    it('disables Create while creating', () => {
      renderPanel({ newSpace: { isCreateSpaceOpen: true, isCreatingSpace: true } });
      expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled();
    });
  });

  describe('space editor', () => {
    beforeEach(() => {
      h.store = { ...h.store, selectedSpaceId: 's-hist' };
    });

    it('opens from Edit space', () => {
      const { editor } = renderPanel();
      fireEvent.click(screen.getByRole('button', { name: 'Edit space' }));
      expect(applyUpdate(editor.setIsSpaceEditorOpen, false)).toBe(true);
    });

    it('edits every field of the selected space', () => {
      const { editor } = renderPanel({ editor: { isSpaceEditorOpen: true } });
      fireEvent.change(screen.getByPlaceholderText('Icon'), { target: { value: '🏛️' } });
      fireEvent.change(screen.getByPlaceholderText('Space name'), { target: { value: 'Antiquity' } });
      fireEvent.change(screen.getByPlaceholderText('#aa503d'), { target: { value: '#112233' } });
      fireEvent.change(screen.getByTitle('Accent color'), { target: { value: '#445566' } });
      fireEvent.change(screen.getByPlaceholderText('Description'), { target: { value: 'Rome and Persia' } });
      fireEvent.change(screen.getByPlaceholderText('System prompt for this space'), { target: { value: 'Cite primary sources' } });
      fireEvent.click(screen.getByRole('button', { name: 'Clear accent color' }));
      expect(editor.setSpaceIconDraft).toHaveBeenCalledWith('🏛️');
      expect(editor.setSpaceNameDraft).toHaveBeenCalledWith('Antiquity');
      expect(editor.setSpaceAccentDraft).toHaveBeenCalledWith('#112233');
      expect(editor.setSpaceAccentDraft).toHaveBeenCalledWith('#445566');
      expect(editor.setSpaceAccentDraft).toHaveBeenLastCalledWith('');
      expect(editor.setSpaceDescriptionDraft).toHaveBeenCalledWith('Rome and Persia');
      expect(editor.setSpacePromptDraft).toHaveBeenCalledWith('Cite primary sources');
    });

    it('falls back to the default accent swatch for an unreadable colour', () => {
      renderPanel({ editor: { isSpaceEditorOpen: true, spaceAccentDraft: 'teal-ish' } });
      expect(screen.getByTitle('Accent color')).toHaveValue('#aa503d');
    });

    it('toggles the research defaults', () => {
      const { editor } = renderPanel({ editor: { isSpaceEditorOpen: true } });
      fireEvent.click(screen.getByRole('button', { name: 'Search documents by default' }));
      fireEvent.click(screen.getByRole('button', { name: 'Web by default' }));
      fireEvent.click(screen.getByRole('button', { name: 'Deep research by default' }));
      expect(applyUpdate(editor.setSpaceKbDefault, true)).toBe(false);
      expect(applyUpdate(editor.setSpaceWebDefault, false)).toBe(true);
      expect(editor.toggleSpaceDeepResearchDefault).toHaveBeenCalled();
      expect(screen.queryByRole('status')).not.toBeInTheDocument();
    });

    it('warns that deep research is slower when it is on', () => {
      renderPanel({ editor: { isSpaceEditorOpen: true, spaceDeepResearchDefault: true } });
      expect(screen.getByRole('status')).toHaveTextContent('Deep research is on for this space. Responses will be slower.');
    });

    it('saves and archives the space', async () => {
      const { editor } = renderPanel({ editor: { isSpaceEditorOpen: true } });
      fireEvent.click(screen.getByRole('button', { name: 'Save' }));
      fireEvent.click(screen.getByRole('button', { name: 'Archive' }));
      await waitFor(() => expect(editor.setSelectedSpaceArchived).toHaveBeenCalledWith(true));
      expect(editor.saveSpaceEnvironment).toHaveBeenCalled();
    });

    it('disables save and archive while they run', () => {
      renderPanel({ editor: { isSpaceEditorOpen: true, isSavingSpace: true, isArchivingSpace: true } });
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(screen.getByRole('button', { name: 'Archive' })).toBeDisabled();
    });

    it('offers restore for an archived selected space', async () => {
      h.store = { ...h.store, selectedSpaceId: 's-old' };
      const { editor } = renderPanel({ editor: { isSpaceEditorOpen: true } });
      const restores = screen.getAllByRole('button', { name: 'Restore' });
      fireEvent.click(restores[0]);
      await waitFor(() => expect(editor.setSelectedSpaceArchived).toHaveBeenCalledWith(false));
    });

    it('never offers to archive General', () => {
      h.store = { ...h.store, selectedSpaceId: GENERAL_SPACE_ID };
      renderPanel({ editor: { isSpaceEditorOpen: true } });
      expect(screen.queryByRole('button', { name: 'Archive' })).not.toBeInTheDocument();
    });

    it('lists archived spaces and restores one', async () => {
      const { editor } = renderPanel({ editor: { isSpaceEditorOpen: true } });
      expect(screen.getByText('Archived spaces · 2')).toBeInTheDocument();
      expect(screen.getByText('⚗️ Old chemistry')).toBeInTheDocument();
      expect(screen.getByText('Old botany')).toBeInTheDocument();
      fireEvent.click(screen.getAllByRole('button', { name: 'Restore' })[1]);
      await waitFor(() => expect(editor.restoreArchivedSpace).toHaveBeenCalledWith('s-old2'));
    });

    it('disables restore while restoring', () => {
      renderPanel({ editor: { isSpaceEditorOpen: true, isRestoringSpace: true } });
      for (const button of screen.getAllByRole('button', { name: 'Restore' })) expect(button).toBeDisabled();
    });
  });
});
