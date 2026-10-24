import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { IndexingSnapshot } from '@/types/api/indexing';

import { IndexingStatusPopover } from './IndexingStatusPopover';

type Callbacks = { onSuccess?: () => void; onError?: (error: Error) => void };

const h = vi.hoisted(() => ({
  navigate: vi.fn(),
  control: { mutate: vi.fn(), isPending: false },
  activities: { data: undefined as unknown[] | undefined },
  activityArgs: [] as unknown[],
  retry: { mutate: vi.fn(), isPending: false },
  remove: { mutate: vi.fn(), isPending: false },
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/features/files/hooks/useIndexingStatusQuery', () => ({
  useIndexingControlMutation: () => h.control,
  useIndexingActivitiesQuery: (...args: unknown[]) => { h.activityArgs = args; return h.activities; },
  useRetryIndexedFileMutation: () => h.retry,
  useRemoveIndexedFileMutation: () => h.remove,
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const failure = (name: string) => ({ path: `/field-notes/${name}`, fileName: name, reason: 'Could not read the PDF', failedAt: '2026-10-20T09:00:00Z' });

const snapshot = (overrides: Partial<IndexingSnapshot> = {}): IndexingSnapshot => ({
  totalFiles: 40,
  processed: 12,
  failed: 0,
  status: 'processing',
  percentage: 30,
  paused: false,
  failures: [],
  ...overrides,
} as IndexingSnapshot);

describe('IndexingStatusPopover', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.control.isPending = false;
    h.retry.isPending = false;
    h.remove.isPending = false;
    h.activities = { data: undefined };
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('says what is indexing, with the current file, and offers pause and stop', () => {
    render(<IndexingStatusPopover snapshot={snapshot({ currentFile: '/field-notes/birds/heron.md' })} open onClose={vi.fn()} />);
    expect(screen.getByText('Indexing 12 of 40')).toBeInTheDocument();
    expect(screen.getByText('heron.md')).toHaveAttribute('title', '/field-notes/birds/heron.md');
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(h.control.mutate.mock.calls).toEqual([['pause'], ['cancel']]);
    expect(h.activityArgs).toEqual([5, true]);
  });

  it('offers resume while paused, and disables controls while one is sent', () => {
    h.control.isPending = true;
    render(<IndexingStatusPopover snapshot={snapshot({ paused: true })} open onClose={vi.fn()} />);
    expect(screen.getByText('Paused · 12 of 40')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Resume' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
  });

  it('shows Nothing indexing when idle with no failures', () => {
    render(<IndexingStatusPopover snapshot={snapshot({ status: 'idle', failed: 0 })} open={false} onClose={vi.fn()} />);
    expect(screen.getByText('Nothing indexing.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Pause' })).not.toBeInTheDocument();
    expect(h.activityArgs).toEqual([5, false]);
  });

  it('leads with the failures when idle, and clears them all', () => {
    const failures = [failure('a.pdf'), failure('b.pdf')];
    render(<IndexingStatusPopover snapshot={snapshot({ status: 'idle', failed: 2, failures })} open onClose={vi.fn()} />);
    expect(screen.getAllByText('2 failed')).toHaveLength(2);
    expect(screen.getByText('a.pdf')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Clear all' }));
    expect(screen.queryByText('a.pdf')).not.toBeInTheDocument();
    expect(screen.getByText('Nothing indexing.')).toBeInTheDocument();
  });

  it('drops a failure once it is retried', () => {
    h.retry.mutate.mockImplementation((_path: string, callbacks: Callbacks) => callbacks.onSuccess?.());
    render(<IndexingStatusPopover snapshot={snapshot({ status: 'complete', failed: 1, failures: [failure('a.pdf')] })} open onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(h.retry.mutate).toHaveBeenCalledWith('/field-notes/a.pdf', expect.anything());
    expect(screen.queryByText('a.pdf')).not.toBeInTheDocument();
  });

  it('brings dismissed failures back when a new scan starts', () => {
    const failures = [failure('a.pdf')];
    const view = render(<IndexingStatusPopover snapshot={snapshot({ status: 'complete', failed: 1, failures })} open onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Clear all' }));
    expect(screen.queryByText('a.pdf')).not.toBeInTheDocument();
    view.rerender(<IndexingStatusPopover snapshot={snapshot({ status: 'scanning', failed: 1, failures })} open onClose={vi.fn()} />);
    expect(screen.getByText('a.pdf')).toBeInTheDocument();
    expect(screen.getByText('Scanning…')).toBeInTheDocument();
  });

  it('lists recent activity and opens the indexing settings', () => {
    vi.useFakeTimers();
    h.activities = {
      data: [
        { id: 1, filePath: '/field-notes/moths.md', timestamp: new Date(Date.now() - 5 * 60_000).toISOString() },
        { id: 2, filePath: 'C:\\notes\\ferns.md', timestamp: 'not a date' },
      ],
    };
    const onClose = vi.fn();
    const listener = vi.fn();
    window.addEventListener('settings:navigate-tab', listener);
    render(<IndexingStatusPopover snapshot={snapshot({ status: 'complete', failed: 0, processed: 40 })} open onClose={onClose} />);
    expect(screen.getByText('Recent activity')).toBeInTheDocument();
    expect(screen.getByText('moths.md')).toBeInTheDocument();
    expect(screen.getByText('5 minutes ago')).toBeInTheDocument();
    expect(screen.getByText('ferns.md')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Show all →' }));
    expect(h.navigate).toHaveBeenCalledWith('/settings');
    expect(onClose).toHaveBeenCalled();
    expect(listener).not.toHaveBeenCalled();
    act(() => { vi.runAllTimers(); });
    expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({ tab: 'indexing' });
    window.removeEventListener('settings:navigate-tab', listener);
  });

  it('shows no activity block for an empty history', () => {
    h.activities = { data: [] };
    render(<IndexingStatusPopover snapshot={snapshot()} open onClose={vi.fn()} />);
    expect(screen.queryByText('Recent activity')).not.toBeInTheDocument();
  });

  it('keeps a path with no separators as its own name', () => {
    render(<IndexingStatusPopover snapshot={snapshot({ currentFile: 'loose-note.md' })} open onClose={vi.fn()} />);
    expect(screen.getByText('loose-note.md')).toBeInTheDocument();
  });
});
