import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useExplorerStore } from '@/features/explorer/stores/explorerStore';
import type { ActivityEntryDto } from '@/lib/bindings';

import { ActivityDrawer } from '../ActivityDrawer';

const mocks = vi.hoisted(() => ({ openDrawer: vi.fn() }));
vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({ openDrawer: mocks.openDrawer }),
}));

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

function Location() {
  const location = useLocation();
  return <output data-testid="location">{`${location.pathname}${location.search}`}</output>;
}

function show(items: ActivityEntryDto[], { readAt = 0, failed = false, onClose = vi.fn() } = {}) {
  render(
    <MemoryRouter initialEntries={['/home']}>
      <Location />
      <ActivityDrawer open onClose={onClose} items={items} readAt={readAt} failed={failed} />
    </MemoryRouter>,
  );
  return { onClose };
}

beforeEach(() => {
  vi.clearAllMocks();
  useExplorerStore.getState().setRoot(null);
});

describe('ActivityDrawer', () => {
  it('says plainly when nothing is running', () => {
    show([]);
    expect(screen.getByRole('status')).toHaveTextContent('Nothing is running.');
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
  });

  it('lists a running item with its progress and the time left it measured', () => {
    show([
      entry({
        detail: 'Embedding passages: 120 of 900',
        progress: { done: 120, total: 900, unit: 'passages' },
        perSecond: 1.1,
        secondsLeft: 12 * 60,
        holding: 'Using the embedding model',
      }),
    ]);
    const row = screen.getByRole('listitem');
    expect(within(row).getByText('Indexing evidence for Calculus')).toBeInTheDocument();
    expect(within(row).getByText('Embedding passages: 120 of 900')).toBeInTheDocument();
    expect(within(row).getByText('about 12 min left')).toBeInTheDocument();
    expect(within(row).getByText('Using the embedding model')).toBeInTheDocument();
    const bar = within(row).getByRole('progressbar', { name: 'Indexing evidence for Calculus progress' });
    expect(bar).toHaveAttribute('aria-valuenow', '13');
    expect(bar).toHaveAttribute('aria-valuetext', '120 of 900 passages');
    expect(screen.getByRole('status')).toHaveTextContent('1 running');
  });

  it('shows no time left when no pace was measured', () => {
    show([entry({ progress: { done: 1, total: 4, unit: 'files' } })]);
    expect(screen.queryByText(/left$/)).not.toBeInTheDocument();
    expect(screen.queryByText('less than a minute')).not.toBeInTheDocument();
  });

  it('says why a paused or waiting item holds', () => {
    show([
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
        retryAt: 3 * 60_000,
      }),
    ]);
    expect(screen.getByText('Paused while Calculus embeds first')).toBeInTheDocument();
    expect(screen.getByText('Queued behind Calculus')).toBeInTheDocument();
    expect(
      screen.getByText('Connection interrupted; saved work will resume automatically. Tries again in about 3 min'),
    ).toBeInTheDocument();
    // A paused item tells no time left; it is not moving.
    expect(screen.queryByText('about 5 min left')).not.toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('1 paused, 2 waiting');
  });

  it('shows the queue on a model', () => {
    show([
      entry({
        id: 'model.requests:server',
        kind: 'model.requests',
        title: 'Qwen3.5-9B',
        detail: '1 request running, 2 queued (1 from a chat)',
      }),
    ]);
    expect(screen.getByText('1 request running, 2 queued (1 from a chat)')).toBeInTheDocument();
    // Nowhere to open: not a button.
    expect(screen.queryByRole('button', { name: /Qwen/ })).not.toBeInTheDocument();
  });

  it('opens the lesson an item prepares, and closes', () => {
    const { onClose } = show([
      entry({ title: 'Preparing “Limits” in Calculus', target: { type: 'lesson', course: 'c-1', lesson: 'l-1' } }),
    ]);
    fireEvent.click(screen.getByRole('button', { name: /Preparing “Limits” in Calculus/ }));
    expect(screen.getByTestId('location')).toHaveTextContent('/studio?course=c-1&lesson=l-1');
    expect(onClose).toHaveBeenCalled();
  });

  it('opens a folder in the Explorer', () => {
    show([entry({ title: 'Indexing notes', target: { type: 'folder', root: '/Users/me/notes', name: 'notes' } })]);
    fireEvent.click(screen.getByRole('button', { name: /Indexing notes/ }));
    expect(useExplorerStore.getState().root).toEqual({ root: '/Users/me/notes', name: 'notes' });
    expect(screen.getByTestId('location')).toHaveTextContent('/explorer');
  });

  it('opens the downloads drawer for a download', () => {
    show([entry({ id: 'download:m', kind: 'download', title: 'Downloading Qwen' })]);
    fireEvent.click(screen.getByRole('button', { name: /Downloading Qwen/ }));
    expect(mocks.openDrawer).toHaveBeenCalled();
  });

  it('says so when the snapshot could not be read', () => {
    show([], { failed: true });
    expect(screen.getByRole('status')).toHaveTextContent('Could not read what is running.');
  });
});
