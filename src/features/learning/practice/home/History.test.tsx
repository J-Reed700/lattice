import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { History } from './History';
import type { HistoryItem } from './historyItems';

const item = (patch: Partial<HistoryItem>): HistoryItem => ({
  id: 'a',
  card: 'code',
  title: 'Two sum',
  when: 1_800_000_000_000,
  detail: 'Coached · Problem',
  result: '3 of 4 passed',
  resumable: false,
  stoppedAt: null,
  target: { kind: 'coding', sessionId: 's1' },
  ...patch,
});

const items = [
  item({}),
  item({ id: 'b', card: 'spot', title: 'Review a parser', target: { kind: 'drill', attemptId: 'att-1' } }),
  item({ id: 'c', card: 'mock', title: 'Acme interview', target: { kind: 'activity', activityId: 'act-1' } }),
];

describe('History', () => {
  it('filters by card and says when nothing is under a filter', async () => {
    const user = userEvent.setup();
    render(<History items={items} onOpen={vi.fn()} onOpenWorkbench={vi.fn()} />);
    expect(screen.getAllByRole('listitem')).toHaveLength(3);
    await user.click(screen.getByRole('button', { name: 'Spot the mistakes' }));
    const rows = screen.getAllByRole('listitem');
    expect(rows).toHaveLength(1);
    expect(within(rows[0]!).getByText('Review a parser')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Role-play' }));
    expect(screen.getByText('Nothing under this filter.')).toBeVisible();
  });

  it('opens an item with its own button', async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    render(<History items={items} onOpen={onOpen} onOpenWorkbench={vi.fn()} />);
    await user.click(screen.getByRole('button', { name: 'Open Acme interview' }));
    expect(onOpen).toHaveBeenCalledWith(items[2]);
  });

  it('opens the practical workbench from Code labs and simulations', async () => {
    const user = userEvent.setup();
    const onOpenWorkbench = vi.fn();
    render(<History items={[]} onOpen={vi.fn()} onOpenWorkbench={onOpenWorkbench} />);
    await user.click(screen.getByRole('button', { name: 'Code labs and simulations' }));
    expect(onOpenWorkbench).toHaveBeenCalled();
  });

  it('shows an error line with a retry', async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    render(<History items={[]} error="Could not load" onRetry={onRetry} onOpen={vi.fn()} onOpenWorkbench={vi.fn()} />);
    expect(screen.getByText(/Could not load/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: /Retry|Try again/ }));
    expect(onRetry).toHaveBeenCalled();
  });
});
