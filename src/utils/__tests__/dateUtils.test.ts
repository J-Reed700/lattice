import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  formatChatTimestamp,
  formatRelativeTime,
  getDateGroup,
  groupBy,
} from '../dateUtils';

const NOW = new Date('2026-10-08T18:00:00');
const MS_PER_DAY = 24 * 60 * 60 * 1000;

describe('date grouping', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it.each([
    [0, 'Today'],
    [1, 'Yesterday'],
    [2, 'This Week'],
    [6, 'This Week'],
    [7, 'Last Week'],
    [13, 'Last Week'],
    [14, 'This Month'],
    [29, 'This Month'],
    [30, 'Older'],
  ] as const)('groups an item from %i days ago as %s', (daysAgo, expected) => {
    expect(getDateGroup(new Date(NOW.getTime() - daysAgo * MS_PER_DAY).toISOString())).toBe(expected);
  });

  it('treats an invalid timestamp as older instead of throwing', () => {
    expect(getDateGroup('not-a-date')).toBe('Older');
  });
});

describe('compact relative time', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it.each([
    [0, 'Just now'],
    [59, 'Just now'],
    [60, '1m ago'],
    [59 * 60, '59m ago'],
    [60 * 60, '1h ago'],
    [23 * 60 * 60, '23h ago'],
    [24 * 60 * 60, '1d ago'],
    [6 * 24 * 60 * 60, '6d ago'],
    [7 * 24 * 60 * 60, '1w ago'],
    [29 * 24 * 60 * 60, '4w ago'],
    [30 * 24 * 60 * 60, '1mo ago'],
    [90 * 24 * 60 * 60, '3mo ago'],
  ] as const)('formats an item from %i seconds ago as %s', (secondsAgo, expected) => {
    const input = new Date(NOW.getTime() - secondsAgo * 1000).toISOString();
    expect(formatRelativeTime(input)).toBe(expected);
  });

  it('treats a future timestamp as just now', () => {
    expect(formatRelativeTime(new Date(NOW.getTime() + MS_PER_DAY).toISOString())).toBe('Just now');
  });
});

describe('chat timestamps', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it.each([
    ['invalid', 'Recently'],
    [new Date(NOW.getTime() - 59_000), 'Just now'],
    [new Date(NOW.getTime() - 60_000), '1m ago'],
    [new Date(NOW.getTime() - 59 * 60_000), '59m ago'],
    [new Date(NOW.getTime() - 60 * 60_000), '1h ago'],
    [new Date(NOW.getTime() - 23 * 60 * 60_000), '23h ago'],
  ])('formats %s as %s', (input, expected) => {
    expect(formatChatTimestamp(input)).toBe(expected);
  });

  it('uses the Yesterday label after crossing the prior calendar day boundary', () => {
    expect(formatChatTimestamp('2026-10-07T17:30:00')).toBe('Yesterday');
  });

  it('uses weekday and time within the past week', () => {
    const input = new Date('2026-10-03T12:15:00');
    expect(formatChatTimestamp(input)).toBe(input.toLocaleDateString(undefined, {
      weekday: 'short',
      hour: 'numeric',
      minute: '2-digit',
    }));
  });

  it('omits the year for an older date in the current year', () => {
    const input = new Date('2026-08-15T12:00:00');
    expect(formatChatTimestamp(input)).toBe(input.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: undefined,
    }));
  });

  it('includes the year for a date from a previous year', () => {
    const input = new Date('2025-12-15T12:00:00');
    expect(formatChatTimestamp(input)).toBe(input.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    }));
  });
});

describe('groupBy', () => {
  it('preserves input order within each generated group', () => {
    const values = [
      { id: 1, kind: 'note' },
      { id: 2, kind: 'pdf' },
      { id: 3, kind: 'note' },
    ];

    expect(groupBy(values, (value) => value.kind)).toEqual({
      note: [values[0], values[2]],
      pdf: [values[1]],
    });
  });

  it('returns an empty object for an empty input', () => {
    expect(groupBy([], String)).toEqual({});
  });
});
