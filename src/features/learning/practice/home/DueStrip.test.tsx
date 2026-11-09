import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { DueStrip } from './DueStrip';

const NOW = new Date(2027, 0, 15, 12, 0).getTime();
const items: Wire.DueItemDto[] = [
  { kind: 'coding_patterns', label: '3 problem patterns', count: 3, dueAt: NOW - 86_400_000, lessonId: null, material: null },
  { kind: 'drill_lesson', label: 'Strings and arrays', count: 1, dueAt: NOW + 3_600_000, lessonId: 'lesson-1', material: 'code' },
];

describe('DueStrip', () => {
  it('lists what is due with its state in words', () => {
    render(<DueStrip items={items} now={NOW} onStart={vi.fn()} />);
    expect(screen.getByText('3 problem patterns')).toBeVisible();
    expect(screen.getByText('Overdue')).toBeVisible();
    expect(screen.getByText('Due today')).toBeVisible();
  });

  it('calls an item that came due earlier today due today, and one from before today overdue', () => {
    const earlier: Wire.DueItemDto[] = [
      { kind: 'drill_lesson', label: 'An hour ago', count: 1, dueAt: NOW - 3_600_000, lessonId: 'l1', material: null },
      { kind: 'drill_lesson', label: 'Yesterday', count: 1, dueAt: new Date(2027, 0, 14, 12, 0).getTime(), lessonId: 'l2', material: null },
    ];
    render(<DueStrip items={earlier} now={NOW} onStart={vi.fn()} />);
    const [hour, yesterday] = screen.getAllByRole('listitem');
    expect(within(hour!).getByText('Due today')).toBeVisible();
    expect(within(yesterday!).getByText('Overdue')).toBeVisible();
  });

  it('starts the most overdue item, the first of the list', async () => {
    const user = userEvent.setup();
    const onStart = vi.fn();
    render(<DueStrip items={items} now={NOW} onStart={onStart} />);
    await user.click(screen.getByRole('button', { name: 'Start the most overdue' }));
    expect(onStart).toHaveBeenCalledWith(items[0]);
  });

  it('says nothing is due and offers no start', () => {
    render(<DueStrip items={[]} now={NOW} onStart={vi.fn()} />);
    expect(screen.getByText(/Nothing is due/)).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Start the most overdue' })).not.toBeInTheDocument();
  });
});
