import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ActivitySnapshotDto } from '@/lib/bindings';

import { ActivityIndicator } from '../ActivityIndicator';

const mocks = vi.hoisted(() => ({
  snapshot: undefined as ActivitySnapshotDto | undefined,
  refresh: vi.fn(),
}));
vi.mock('../../api', () => ({
  useActivitySnapshot: () => ({ snapshot: mocks.snapshot, readAt: 0, failed: false, refresh: mocks.refresh }),
}));
vi.mock('@/features/model/hooks/useDownloadState', () => ({ useDownloadState: () => ({ openDrawer: vi.fn() }) }));

function show() {
  render(
    <MemoryRouter>
      <ActivityIndicator />
    </MemoryRouter>,
  );
}

const running = {
  id: 'a',
  kind: 'download',
  title: 'Downloading Qwen',
  state: 'running',
  detail: null,
  progress: null,
  perSecond: null,
  secondsLeft: null,
  holding: null,
  target: null,
  since: null,
  retryAt: null,
} as const;

beforeEach(() => {
  vi.clearAllMocks();
  mocks.snapshot = undefined;
});

describe('ActivityIndicator', () => {
  it('is always there, quiet while nothing runs', () => {
    mocks.snapshot = { items: [], running: 0 };
    show();
    expect(screen.getByRole('button', { name: 'Activity: nothing running' })).toBeInTheDocument();
    expect(screen.queryByTestId('activity-busy-dot')).not.toBeInTheDocument();
  });

  it('counts what runs and shows a busy dot', () => {
    mocks.snapshot = { items: [running, { ...running, id: 'b' }], running: 2 };
    show();
    expect(screen.getByRole('button', { name: 'Activity: 2 running' })).toBeInTheDocument();
    expect(screen.getByTestId('activity-busy-dot')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('reads again and opens the drawer when pressed', () => {
    mocks.snapshot = { items: [], running: 0 };
    show();
    fireEvent.click(screen.getByRole('button', { name: 'Activity: nothing running' }));
    expect(mocks.refresh).toHaveBeenCalled();
    expect(screen.getByRole('dialog', { name: 'Activity' })).toBeInTheDocument();
  });
});
