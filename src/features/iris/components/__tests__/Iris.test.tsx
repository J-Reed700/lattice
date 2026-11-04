import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { useExplorerStore } from '@/features/explorer/stores/explorerStore';
import { usePaletteCommandsStore } from '@/features/palette/stores/paletteCommandsStore';
import type { ActivityEntryDto } from '@/lib/bindings';

import { readIris, type IrisInputs } from '../../model/irisReading';
import { useIrisStore } from '../../stores/irisStore';
import { Iris } from '../Iris';

const mocks = vi.hoisted(() => ({
  inputs: null as Omit<IrisInputs, 'seenNoteIds'> | null,
  read: vi.fn(),
  subscribe: vi.fn(),
  refresh: vi.fn(),
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
vi.mock('@/features/activity/api', () => ({
  useActivitySnapshot: () => {
    mocks.subscribe();
    return { snapshot: undefined, readAt: 0, failed: false, refresh: mocks.refresh };
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

function entry(overrides: Partial<ActivityEntryDto> = {}): ActivityEntryDto {
  return {
    id: 'job-1',
    kind: 'learning.course_index',
    title: 'Indexing evidence for Calculus',
    state: 'running',
    detail: null,
    progress: null,
    perSecond: null,
    secondsLeft: null,
    holding: null,
    target: null,
    since: null,
    retryAt: null,
    ...overrides,
  };
}

function withWork(items: ActivityEntryDto[]) {
  mocks.inputs = {
    ...mocks.inputs!,
    activity: { items, running: items.filter((item) => item.state === 'running').length },
  };
}

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

async function openCard() {
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: /^Iris:/ }));
  const card = await screen.findByRole('dialog');
  // The card loads on first open.
  await within(card).findByRole('status');
  return { user, card };
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  useIrisStore.setState({ enabled: true, seenNoteIds: new Set() });
  usePaletteCommandsStore.setState({ commands: new Map() });
  useExplorerStore.getState().setRoot(null);
  mocks.inputs = {
    activity: { items: [], running: 0 },
    activityFailed: false,
    indexingFailures: 0,
    failedDownloads: [],
    dueCards: 0,
  };
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
    expect(mocks.subscribe).not.toHaveBeenCalled();
  });
});

describe('Iris as the one place for what is running', () => {
  it('keeps the activity snapshot current while she is shown', () => {
    show();
    expect(mocks.subscribe).toHaveBeenCalled();
  });

  it('reads again each time the card opens, for anything that moved without an event', async () => {
    show();
    expect(mocks.refresh).not.toHaveBeenCalled();
    const { user } = await openCard();
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
    await user.keyboard('{Escape}');
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('button', { name: /^Iris:/ }));
    expect(mocks.refresh).toHaveBeenCalledTimes(2);
  });

  it('counts what runs on the rail and tells a screen reader as it starts and stops', () => {
    withWork([entry(), entry({ id: 'b' }), entry({ id: 'c', state: 'waiting' })]);
    show();
    const button = screen.getByRole('button', { name: /^Iris: Working on 2 things\./ });
    expect(within(button).getByTestId('iris-orb')).toHaveAttribute('data-mood', 'working');
    expect(screen.getByText('Iris: 2 running')).toHaveAttribute('role', 'status');
  });

  it('says plainly when nothing is running, and what will appear here', async () => {
    show();
    const { card } = await openCard();
    expect(within(card).getByText('Nothing is running.')).toBeInTheDocument();
    expect(
      within(card).getByText('Lessons, indexes, model requests and downloads appear here while they run.'),
    ).toBeInTheDocument();
    expect(within(card).queryByRole('list')).not.toBeInTheDocument();
  });

  it('lists everything that runs, not only the first few', async () => {
    const items = Array.from({ length: 6 }, (_, index) => entry({ id: `job-${index}`, title: `Task ${index + 1}` }));
    withWork(items);
    show();
    const { card } = await openCard();
    for (const item of items) expect(within(card).getByText(item.title)).toBeInTheDocument();
    expect(within(card).getByText('6 running')).toBeInTheDocument();
    expect(within(card).queryByText(/more in Activity/)).not.toBeInTheDocument();
  });

  it('says what an item is doing, how far it has got and what it holds', async () => {
    withWork([
      entry({
        detail: 'Embedding passages: 120 of 900',
        progress: { done: 120, total: 900, unit: 'passages' },
        perSecond: 1.1,
        secondsLeft: 12 * 60,
        holding: 'Using the embedding model',
      }),
    ]);
    show();
    const { card } = await openCard();
    const row = within(card).getByRole('listitem');
    expect(within(row).getByText('Indexing evidence for Calculus')).toBeInTheDocument();
    expect(within(row).getByText('Running:')).toBeInTheDocument();
    expect(within(row).getByText('Embedding passages: 120 of 900')).toBeInTheDocument();
    expect(within(row).getByText('13%')).toBeInTheDocument();
    expect(within(row).getByText('120 of 900 passages')).toBeInTheDocument();
    expect(within(row).getByText('about 12 min left')).toBeInTheDocument();
    expect(within(row).getByText('Using the embedding model')).toBeInTheDocument();
    const bar = within(row).getByRole('progressbar', { name: 'Indexing evidence for Calculus progress' });
    expect(bar).toHaveAttribute('aria-valuenow', '13');
    expect(bar).toHaveAttribute('aria-valuetext', '120 of 900 passages');
    expect(within(card).getByText('1 running')).toBeInTheDocument();
  });

  it('shows no time left when no pace was measured', async () => {
    withWork([entry({ progress: { done: 1, total: 4, unit: 'files' } })]);
    show();
    const { card } = await openCard();
    expect(within(card).getByText('1 of 4 files')).toBeInTheDocument();
    expect(within(card).queryByText(/left$/)).not.toBeInTheDocument();
    expect(within(card).queryByText('less than a minute')).not.toBeInTheDocument();
  });

  it('says why a paused or waiting item holds, and when a deferred one tries again', async () => {
    withWork([
      entry({
        id: 'folder',
        title: 'Indexing notes',
        state: 'paused',
        detail: 'Paused while Calculus embeds first',
        progress: { done: 10, total: 100, unit: 'passages' },
        secondsLeft: 300,
      }),
      entry({ id: 'queued', title: 'Indexing evidence for Biology', state: 'waiting', detail: 'Queued behind Calculus' }),
      entry({
        id: 'deferred',
        title: 'Preparing “Limits”',
        state: 'waiting',
        detail: 'Connection interrupted; saved work will resume automatically',
        retryAt: Date.now() + 3 * 60_000 + 5_000,
      }),
    ]);
    show();
    const { card } = await openCard();
    expect(within(card).getByText('Paused while Calculus embeds first')).toBeInTheDocument();
    expect(within(card).getByText('Queued behind Calculus')).toBeInTheDocument();
    expect(
      within(card).getByText('Connection interrupted; saved work will resume automatically. Tries again in about 3 min'),
    ).toBeInTheDocument();
    expect(within(card).getByText('Paused:')).toBeInTheDocument();
    expect(within(card).getAllByText('Waiting:')).toHaveLength(2);
    // A paused item is not moving, so it tells no time left.
    expect(within(card).queryByText('about 5 min left')).not.toBeInTheDocument();
    expect(within(card).getByText('1 paused, 2 waiting')).toBeInTheDocument();
  });

  it('shows the queue on a model, which has nowhere to open', async () => {
    withWork([
      entry({
        id: 'model.requests:server',
        kind: 'model.requests',
        title: 'Qwen3.5-9B',
        detail: '1 request running, 2 queued (1 from a chat)',
      }),
    ]);
    show();
    const { card } = await openCard();
    expect(within(card).getByText('1 request running, 2 queued (1 from a chat)')).toBeInTheDocument();
    expect(within(card).queryByRole('button', { name: /Qwen/ })).not.toBeInTheDocument();
  });

  it.each([
    ['a course', { type: 'course', course: 'c-1' }, '/studio?course=c-1'],
    ['a lesson', { type: 'lesson', course: 'c-1', lesson: 'l-1' }, '/studio?course=c-1&lesson=l-1'],
    ['a chat', { type: 'conversation', conversation: 'chat-1' }, '/chat?conversationId=chat-1'],
  ] as const)('opens %s from the item that works on it, and closes the card', async (_name, target, where) => {
    withWork([entry({ title: 'Preparing “Limits” in Calculus', target })]);
    show();
    const { user, card } = await openCard();
    await user.click(within(card).getByRole('button', { name: /Preparing “Limits” in Calculus/ }));
    expect(screen.getByTestId('where')).toHaveTextContent(where);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens a folder in the Explorer', async () => {
    withWork([entry({ title: 'Indexing notes', target: { type: 'folder', root: '/notes', name: 'notes' } })]);
    show();
    const { user, card } = await openCard();
    await user.click(within(card).getByRole('button', { name: /Indexing notes/ }));
    expect(useExplorerStore.getState().root).toEqual({ root: '/notes', name: 'notes' });
    expect(screen.getByTestId('where')).toHaveTextContent('/explorer');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens the downloads drawer from a download that is running', async () => {
    withWork([download]);
    show();
    const { user, card } = await openCard();
    await user.click(within(card).getByRole('button', { name: /Downloading Qwen3-8B/ }));
    expect(mocks.openDrawer).toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('says so when what is running could not be read, rather than calling it quiet', async () => {
    mocks.inputs = { ...mocks.inputs!, activity: undefined, activityFailed: true };
    show();
    const orb = () => screen.getAllByTestId('iris-orb')[0];
    expect(orb()).toHaveAttribute('data-mood', 'notice');
    expect(orb()).toHaveAttribute('data-tone', 'alert');

    const { user, card } = await openCard();
    expect(within(card).getAllByText('Could not read what is running.')).toHaveLength(2);
    expect(within(card).queryByText('Nothing is running.')).not.toBeInTheDocument();
    expect(within(card).queryByRole('heading', { name: 'Working on' })).not.toBeInTheDocument();

    await user.keyboard('{Escape}');
    expect(screen.getByRole('button', { name: /^Iris: Could not read what is running\./ })).toBeInTheDocument();
    expect(orb()).toHaveAttribute('data-mood', 'idle');
  });

  it('keeps listing what it last read while a later read fails', async () => {
    withWork([entry()]);
    mocks.inputs = { ...mocks.inputs!, activityFailed: true };
    show();
    const { card } = await openCard();
    expect(within(card).getByText('Indexing evidence for Calculus')).toBeInTheDocument();
    expect(within(card).getByText('Could not read what is running.')).toBeInTheDocument();
  });
});
