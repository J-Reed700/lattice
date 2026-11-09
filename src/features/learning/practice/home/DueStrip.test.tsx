import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { DueStrip } from './DueStrip';

const NOW = 1_800_000_000_000;
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
