import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { usePaletteCommandsStore } from '@/features/palette/stores/paletteCommandsStore';
import type { ActivityEntryDto } from '@/lib/bindings';

import { readIris, type IrisInputs } from '../../model/irisReading';
import { useIrisStore } from '../../stores/irisStore';
import { Iris } from '../Iris';

const mocks = vi.hoisted(() => ({
  inputs: null as Omit<IrisInputs, 'seenNoteIds'> | null,
  read: vi.fn(),
  openDrawer: vi.fn(),
}));

// The real derivation, over inputs each test sets; seen notes come from the real store.
vi.mock('../../hooks/useIrisReading', () => ({
  useIrisReading: () => {
    mocks.read();
    const seenNoteIds = useIrisStore((state) => state.seenNoteIds);
    return readIris({ ...mocks.inputs!, seenNoteIds });
  },
}));
vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({ openDrawer: mocks.openDrawer }),
}));

const download: ActivityEntryDto = {
  id: 'dl',
  kind: 'download',
  title: 'Downloading Qwen3-8B',
  state: 'running',
  detail: '1.2 GB of 4.5 GB',
  progress: { done: 30, total: 100, unit: 'bytes' },
  perSecond: null,
  secondsLeft: null,
  holding: null,
  target: null,
  since: null,
  retryAt: null,
};

function Where() {
  const location = useLocation();
  return <output data-testid="where">{location.pathname + location.search}</output>;
}

function show(compact = true) {
  return render(
    <TooltipProvider>
      <MemoryRouter initialEntries={['/home']}>
        <Routes>
          <Route path="*" element={<><Iris compact={compact} /><Where /></>} />
        </Routes>
      </MemoryRouter>
    </TooltipProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  useIrisStore.setState({ enabled: true, seenNoteIds: new Set() });
  usePaletteCommandsStore.setState({ commands: new Map() });
  mocks.inputs = { activity: { items: [], running: 0 }, indexingFailures: 0, failedDownloads: [], dueCards: 0 };
});

describe('Iris', () => {
  it('rests quietly in the rail when there is nothing to say', () => {
    show();
    const button = screen.getByRole('button', { name: 'Iris: All quiet. Nothing is running.' });
    expect(within(button).getByTestId('iris-orb')).toHaveAttribute('data-mood', 'idle');
  });

  it('shows its status beside the name on a labeled rail', () => {
    show(false);
    expect(screen.getByText('Quiet')).toBeInTheDocument();
  });

  it('opens a card with what is running and what needs a look', async () => {
    const user = userEvent.setup();
    mocks.inputs = { ...mocks.inputs!, activity: { items: [download], running: 1 }, dueCards: 3 };
    show();

    const button = screen.getByRole('button', { name: /^Iris: Downloading Qwen3-8B, 30% done\./ });
    expect(within(button).getByTestId('iris-orb')).toHaveAttribute('data-mood', 'working');
    await user.click(button);

    const card = await screen.findByRole('dialog');
    // The card loads on first open.
    expect(await within(card).findByRole('status')).toHaveTextContent('Downloading Qwen3-8B, 30% done.');
    expect(within(card).getByText('3 flashcards are due for review.')).toBeInTheDocument();
    expect(within(card).getByRole('button', { name: /Downloading Qwen3-8B/ })).toBeInTheDocument();
    expect(within(card).getByText('30%')).toBeInTheDocument();
  });

  it('takes a note to where it can be dealt with', async () => {
    const user = userEvent.setup();
    mocks.inputs = { ...mocks.inputs!, dueCards: 3 };
    show();
    await user.click(screen.getByRole('button', { name: /^Iris:/ }));
    await user.click(await screen.findByRole('button', { name: 'Review' }));
    expect(screen.getByTestId('where')).toHaveTextContent('/studio?section=flashcards');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens the downloads drawer for a failed download', async () => {
    const user = userEvent.setup();
    mocks.inputs = { ...mocks.inputs!, failedDownloads: [{ id: 'd1', name: 'MiniLM' }] };
    show();
    await user.click(screen.getByRole('button', { name: /^Iris: The MiniLM download failed\./ }));
    await user.click(await screen.findByRole('button', { name: 'Open downloads' }));
    expect(mocks.openDrawer).toHaveBeenCalled();
  });

  it('settles once you have seen what it noticed', async () => {
    const user = userEvent.setup();
    mocks.inputs = { ...mocks.inputs!, indexingFailures: 2 };
    show();
    const orb = () => screen.getAllByTestId('iris-orb')[0];
    expect(orb()).toHaveAttribute('data-mood', 'notice');
    expect(orb()).toHaveAttribute('data-tone', 'alert');

    await user.click(screen.getByRole('button', { name: /^Iris:/ }));
    // Still as it was while the card is open.
    expect(orb()).toHaveAttribute('data-tone', 'alert');
    await user.keyboard('{Escape}');

    expect(orb()).toHaveAttribute('data-mood', 'idle');
    expect(orb()).toHaveAttribute('data-tone', 'calm');
  });

  it('can be hidden, and brought back from the palette', async () => {
    const user = userEvent.setup();
    show();
    await user.click(screen.getByRole('button', { name: /^Iris:/ }));
    await user.click(await screen.findByRole('button', { name: 'Hide' }));

    expect(screen.queryByRole('button', { name: /^Iris:/ })).not.toBeInTheDocument();
    expect(localStorage.getItem('iris.enabled')).toBe('0');

    const commands = usePaletteCommandsStore.getState().commands;
    expect(commands.get('iris.hide')?.enabled).toBe(false);
    const showCommand = commands.get('iris.show');
    expect(showCommand?.enabled).toBe(true);
    await showCommand!.run();
    expect(await screen.findByRole('button', { name: /^Iris:/ })).toBeInTheDocument();
  });

  it('reads nothing while hidden', () => {
    useIrisStore.setState({ enabled: false });
    show();
    expect(screen.queryByRole('button', { name: /^Iris:/ })).not.toBeInTheDocument();
    expect(mocks.read).not.toHaveBeenCalled();
  });
});
