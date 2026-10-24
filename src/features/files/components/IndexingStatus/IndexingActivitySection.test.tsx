import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { IndexingActivitySection } from './IndexingActivitySection';

const h = vi.hoisted(() => ({
  snapshot: undefined as unknown,
  activities: undefined as unknown,
  control: { mutate: vi.fn(), isPending: false },
}));
vi.mock('@/features/files/hooks/useIndexingStatusQuery', () => ({
  useIndexingStatusQuery: () => ({ data: h.snapshot }),
  useIndexingActivitiesQuery: () => ({ data: h.activities }),
  useIndexingControlMutation: () => h.control,
}));
vi.mock('./FailedItemsList', () => ({ FailedItemsList: ({ failures, limit }: { failures: unknown[]; limit: number }) => <p>{failures.length} failures, up to {limit}</p> }));

const snapshot = (overrides: Record<string, unknown> = {}) => ({
  totalFiles: 20, processed: 5, failed: 0, status: 'processing', percentage: 25, paused: false, failures: [], ...overrides,
});

describe('IndexingActivitySection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.snapshot = undefined;
    h.activities = undefined;
    h.control.isPending = false;
  });

  it('says nothing is indexing yet', () => {
    render(<IndexingActivitySection />);
    expect(screen.getByText('Nothing indexing.')).toBeInTheDocument();
    expect(screen.getByText('Nothing indexed yet. Add a folder to start.')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('pauses or stops a running index', () => {
    h.snapshot = snapshot();
    render(<IndexingActivitySection />);
    expect(screen.getByText('Indexing 5 of 20')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(h.control.mutate.mock.calls).toEqual([['pause'], ['cancel']]);
  });

  it('resumes a paused index, and waits while a command is sent', () => {
    h.snapshot = snapshot({ status: 'idle', paused: true });
    h.control.isPending = true;
    render(<IndexingActivitySection />);
    expect(screen.getByRole('button', { name: 'Resume' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
  });

  it('resumes when asked', () => {
    h.snapshot = snapshot({ status: 'scanning', paused: true });
    render(<IndexingActivitySection />);
    fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
    expect(h.control.mutate).toHaveBeenCalledWith('resume');
  });

  it('lists failures and recent files', () => {
    h.snapshot = snapshot({ status: 'complete', failed: 2, processed: 20, failures: [{ path: '/a.pdf' }, { path: '/b.pdf' }] });
    h.activities = [
      { id: 1, filePath: '/field-notes/moths.md', timestamp: new Date(Date.now() - 3 * 60_000).toISOString() },
      { id: 2, filePath: 'C:\\notes\\', timestamp: 'never' },
    ];
    render(<IndexingActivitySection />);
    expect(screen.getByText('2 failed')).toBeInTheDocument();
    expect(screen.getByText('2 failures, up to 10')).toBeInTheDocument();
    expect(screen.getByText('moths.md')).toHaveAttribute('title', '/field-notes/moths.md');
    expect(screen.getByText('3 minutes ago')).toBeInTheDocument();
    expect(screen.getByText('C:\\notes\\')).toBeInTheDocument();
  });
});
