import { act, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useRestoreLockoutStore } from '@/features/backup/restoreLockoutStore';
import { useJournalShortcuts } from '@/features/journal/hooks/useJournalCommands';
import { useJournalPageActions } from '@/features/journal/hooks/useJournalPageActions';
import { usePaletteCommandsStore } from '@/features/palette/stores/paletteCommandsStore';
import { useEditMode } from '@/shared/edits/editMode';
import { NEW_ITEM_EVENT } from '@/shared/newItemEvent';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { RootLayout } from '../RootLayout';

const mocks = vi.hoisted(() => ({
  syntheses: { data: [] as unknown[] },
  savedSynthesis: false,
  readClipboardText: vi.fn(),
  deepResearch: vi.fn(),
}));

vi.mock('@/features/journal/synthesis/api', () => ({ useJournalSyntheses: () => mocks.syntheses }));
vi.mock('@/features/journal/synthesis/synthesisPanel', () => ({
  useSynthesisPanel: (select: (state: { saved: unknown }) => unknown) => select({ saved: mocks.savedSynthesis ? {} : null }),
}));
vi.mock('@/features/journal/synthesis/SynthesisProgress', () => ({ SynthesisProgress: () => <p>Synthesis in progress</p> }));
vi.mock('@/features/jobs/research', () => ({ useDeepResearchJobs: mocks.deepResearch }));
vi.mock('@/features/palette/components/CommandPalette', () => ({ CommandPalette: () => null }));
vi.mock('@/utils/clipboard', () => ({ readClipboardText: mocks.readClipboardText }));
vi.mock('@/features/journal/components/QuickCaptureDialog', () => ({
  QuickCaptureDialog: ({ open, clipboard, onClose, onOpenPage }: { open: boolean; clipboard: string | null; onClose: () => void; onOpenPage: (id: string) => void }) =>
    open ? (
      <div role="dialog" aria-label="Quick capture">
        <p>Clipboard: {clipboard ?? 'none'}</p>
        <button type="button" onClick={() => onOpenPage('note 1')}>Open page</button>
        <button type="button" onClick={onClose}>Close capture</button>
      </div>
    ) : null,
}));

function Location() {
  const location = useLocation();
  return <output aria-label="location">{location.pathname + location.search}</output>;
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<RootLayout />}>
          <Route path="*" element={<Location />} />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
}

const location = () => screen.getByRole('status', { name: 'location' }).textContent;
// Shortcuts answer key presses from page elements, which bubble up to the window.
const press = (key: string, extra: KeyboardEventInit = {}) => act(() => { fireEvent.keyDown(document.body, { key, ctrlKey: true, ...extra }); });

beforeEach(() => {
  useRestoreLockoutStore.getState().resetRestoreLockout();
  mocks.syntheses = { data: [] };
  mocks.savedSynthesis = false;
  mocks.readClipboardText.mockReset().mockResolvedValue(null);
  usePaletteCommandsStore.setState({ commands: new Map() });
  useEditMode.setState({ on: false });
});
afterEach(() => vi.restoreAllMocks());

describe('RootLayout', () => {
  it.each([
    ['0', '/home'],
    ['1', '/search'],
    ['2', '/files'],
    ['3', '/journals'],
    ['4', '/chat'],
    ['5', '/references'],
    ['6', '/explorer'],
    ['7', '/studio'],
    ['i', '/ingest'],
    [',', '/settings'],
  ])('goes where Ctrl+%s points', (key, path) => {
    renderAt('/home');
    press(key);
    expect(location()).toBe(path);
  });

  it('starts something new where it makes sense', () => {
    renderAt('/files');
    press('n');
    expect(location()).toBe('/chat?new=1');
  });

  it('starts a new course from Studio', () => {
    renderAt('/studio');
    press('n');
    expect(location()).toBe('/studio?new=1');
  });

  it('lets Chat and Journal start their own new item', () => {
    const onNew = vi.fn();
    window.addEventListener(NEW_ITEM_EVENT, onNew);
    renderAt('/journals');
    press('n');
    expect(location()).toBe('/journals');
    expect((onNew.mock.calls[0][0] as CustomEvent).detail).toEqual({ pathname: '/journals' });
    window.removeEventListener(NEW_ITEM_EVENT, onNew);
  });

  it('opens quick capture with the clipboard as a hint, and opens the saved page', async () => {
    mocks.readClipboardText.mockResolvedValue('Knead for ten minutes.');
    renderAt('/home');
    await act(async () => { fireEvent.keyDown(document.body, { key: 'N', ctrlKey: true, shiftKey: true }); });
    expect(await screen.findByRole('dialog', { name: 'Quick capture' })).toHaveTextContent('Clipboard: Knead for ten minutes.');
    fireEvent.click(screen.getByRole('button', { name: 'Open page' }));
    expect(location()).toBe('/journals?noteId=note+1');
  });

  it('ignores a clipboard read that lands after capture closed', async () => {
    let finish!: (text: string) => void;
    mocks.readClipboardText.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    renderAt('/home');
    press('N', { shiftKey: true });
    expect(await screen.findByRole('dialog')).toHaveTextContent('Clipboard: none');
    fireEvent.click(screen.getByRole('button', { name: 'Close capture' }));
    await act(async () => { finish('late text'); });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('offers quick capture in the palette', async () => {
    renderAt('/home');
    const command = usePaletteCommandsStore.getState().commands.get('vault.quickCapture')!;
    expect(command).toMatchObject({ label: 'Quick capture', group: 'Vault', shortcut: '⌘⇧N' });
    await act(async () => { command.run(); });
    expect(await screen.findByRole('dialog', { name: 'Quick capture' })).toBeInTheDocument();
  });

  it('shows synthesis progress only while there is a synthesis', async () => {
    const { unmount } = renderAt('/home');
    expect(screen.queryByText('Synthesis in progress')).not.toBeInTheDocument();
    expect(mocks.deepResearch).toHaveBeenCalled();
    unmount();
    mocks.syntheses = { data: [{ id: 'job-1' }] };
    renderAt('/home');
    expect(await screen.findByText('Synthesis in progress')).toBeInTheDocument();
  });

  it('shows a saved synthesis after its job is gone', async () => {
    mocks.savedSynthesis = true;
    renderAt('/home');
    expect(await screen.findByText('Synthesis in progress')).toBeInTheDocument();
  });

  it('locks the app behind an accessible restart-required modal after database restore and blocks global shortcuts', async () => {
    renderAt('/home');
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();

    act(() => {
      useRestoreLockoutStore.getState().markDatabaseRestoredRestartRequired(false);
    });

    const lockout = await screen.findByRole('alertdialog');
    expect(lockout).toHaveAttribute('aria-modal', 'true');
    expect(lockout).toHaveFocus();
    expect(location()).toBe('/home');
    expect(screen.getByRole('status', { name: 'location' }).closest('[inert]')).not.toBeNull();
    expect(lockout).toHaveTextContent('Restart required');
    expect(lockout).toHaveTextContent('Quit Lattice and open it again to use the restored library.');

    const tabEvent = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    act(() => { lockout.dispatchEvent(tabEvent); });
    expect(tabEvent.defaultPrevented).toBe(true);

    // Global shortcuts must not navigate or open Quick Capture while locked out.
    press('2');
    expect(location()).toBe('/home');
    press('N', { shiftKey: true });
    expect(screen.queryByRole('dialog', { name: 'Quick capture' })).not.toBeInTheDocument();
  });

  it('blocks window keydown handlers and marks portaled dialogs inert while restore lockout is active', async () => {
    const windowKeydown = vi.fn();
    window.addEventListener('keydown', windowKeydown);
    const portalHost = document.createElement('div');
    portalHost.setAttribute('role', 'dialog');
    portalHost.textContent = 'Portaled dialog';
    document.body.appendChild(portalHost);

    try {
      renderAt('/home');
      act(() => {
        useRestoreLockoutStore.getState().markDatabaseRestoredRestartRequired(false, true);
      });

      const lockout = await screen.findByRole('alertdialog');
      expect(lockout).toHaveTextContent(
        'Restore failed after closing the library connection. Quit Lattice and open it again to reconnect.',
      );
      expect(portalHost.hasAttribute('inert')).toBe(true);

      act(() => {
        lockout.dispatchEvent(new KeyboardEvent('keydown', { key: '1', bubbles: true, cancelable: true }));
      });
      expect(windowKeydown).not.toHaveBeenCalled();

      act(() => {
        useRestoreLockoutStore.getState().resetRestoreLockout();
      });
      expect(portalHost.hasAttribute('inert')).toBe(false);
    } finally {
      window.removeEventListener('keydown', windowKeydown);
      portalHost.remove();
    }
  });

  it('creates exactly one journal page on Mod+N on /journals both outside and inside an editor', async () => {
    const createPage = vi.fn().mockResolvedValue({ id: 'new-page', title: 'Untitled page' } as WorkspaceNote);
    function JournalHarness() {
      useJournalPageActions({
        journalSpace: { id: 'j1', name: 'Garden' } as never,
        requestedJournalSpaceId: 'j1',
        isLoadingNote: false,
        selectPage: vi.fn().mockResolvedValue(undefined),
        createPage,
        renamePage: vi.fn().mockResolvedValue(true),
        deletePage: vi.fn().mockResolvedValue(true),
        notify: vi.fn(),
        searchParams: new URLSearchParams(),
        setSearchParams: vi.fn(),
      });
      useJournalShortcuts({
        entries: [],
        selectedId: null,
        setSelectedId: vi.fn(),
        saveNow: vi.fn().mockResolvedValue(true),
        setSidebarCollapsed: vi.fn(),
      });
      return <textarea aria-label="Editor" />;
    }

    render(
      <MemoryRouter initialEntries={['/journals?journalSpaceId=j1']}>
        <Routes>
          <Route element={<RootLayout />}>
            <Route path="/journals" element={<JournalHarness />} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );

    press('n');
    expect(createPage).toHaveBeenCalledTimes(1);

    await act(async () => {
      fireEvent.keyDown(screen.getByLabelText('Editor'), { key: 'n', ctrlKey: true, bubbles: true });
    });
    expect(createPage).toHaveBeenCalledTimes(2);
  });

  it('toggles edit mode on Mod+Shift+E and registers Edit mode in the command palette', () => {
    renderAt('/home');
    expect(useEditMode.getState().on).toBe(false);

    press('E', { shiftKey: true });
    expect(useEditMode.getState().on).toBe(true);

    const command = usePaletteCommandsStore.getState().commands.get('learning.editMode');
    expect(command?.label).toBe('Edit mode');
    act(() => command?.run());
    expect(useEditMode.getState().on).toBe(false);
  });
});
