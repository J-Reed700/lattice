import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { formatBytes, formatRelativeTime } from '../formatters';

describe('file sizes from incomplete download metadata', () => {
  it.each([
    [0, '0 B'], [1, '1 B'], [0.5, '0.5 B'], [1023, '1023 B'],
    [1024, '1 KB'], [1536, '1.5 KB'], [1024 ** 2, '1 MB'],
    [1024 ** 3, '1 GB'], [1024 ** 4, '1 TB'], [1024 ** 5, '1 PB'],
    [-1, '0 B'], [NaN, '0 B'], [Infinity, '0 B'], [-Infinity, '0 B'],
  ])('renders %s bytes as %s', (bytes, expected) => { expect(formatBytes(bytes)).toBe(expected); });
});

describe('relative timestamps', () => {
  beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(new Date('2026-10-08T12:00:00Z')); });
  afterEach(() => { vi.useRealTimers(); });
  it.each([
    [undefined, 'Recently'], ['', 'Recently'], ['Never', 'Never'],
    ['not-a-date', 'Recently'], ['2026-10-09T00:00:00Z', 'Just now'],
    ['2026-10-08T11:59:01Z', 'Just now'], ['2026-10-08T11:59:00Z', '1m ago'],
    ['2026-10-08T11:00:00Z', '1h ago'], ['2026-10-07T12:00:00Z', '1d ago'],
  ])('renders %s as %s', (date, expected) => { expect(formatRelativeTime(date)).toBe(expected); });
  it('uses the caller fallback for invalid dates and local date formatting for older entries', () => {
    expect(formatRelativeTime('invalid', 'Unknown')).toBe('Unknown');
    const older = '2026-09-01T12:00:00Z';
    expect(formatRelativeTime(older)).toBe(new Date(older).toLocaleDateString());
  });
});
