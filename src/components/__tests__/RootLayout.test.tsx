import { act, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { usePaletteCommandsStore } from '@/features/palette/stores/paletteCommandsStore';
import { NEW_ITEM_EVENT } from '@/shared/newItemEvent';

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
  mocks.syntheses = { data: [] };
  mocks.savedSynthesis = false;
  mocks.readClipboardText.mockReset().mockResolvedValue(null);
  usePaletteCommandsStore.setState({ commands: new Map() });
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
    expect(screen.getByRole('dialog', { name: 'Quick capture' })).toHaveTextContent('Clipboard: Knead for ten minutes.');
    fireEvent.click(screen.getByRole('button', { name: 'Open page' }));
    expect(location()).toBe('/journals?noteId=note+1');
  });

  it('ignores a clipboard read that lands after capture closed', async () => {
    let finish!: (text: string) => void;
    mocks.readClipboardText.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    renderAt('/home');
    press('N', { shiftKey: true });
    expect(screen.getByRole('dialog')).toHaveTextContent('Clipboard: none');
    fireEvent.click(screen.getByRole('button', { name: 'Close capture' }));
    await act(async () => { finish('late text'); });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('offers quick capture in the palette', async () => {
    renderAt('/home');
    const command = usePaletteCommandsStore.getState().commands.get('vault.quickCapture')!;
    expect(command).toMatchObject({ label: 'Quick capture', group: 'Vault', shortcut: '⌘⇧N' });
    await act(async () => { command.run(); });
    expect(screen.getByRole('dialog', { name: 'Quick capture' })).toBeInTheDocument();
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
});
