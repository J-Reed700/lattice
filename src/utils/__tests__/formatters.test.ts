import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  formatBytes,
  formatEta,
  formatFileSize,
  formatRelativeDate,
  formatRelativeTime,
  formatTimeLeft,
  indexPercent,
  progressPercent,
} from '../formatters';

describe('file sizes from incomplete download metadata', () => {
  it.each([
    [0, '0 B'], [1, '1 B'], [0.5, '0.5 B'], [1023, '1023 B'],
    [1024, '1 KB'], [1536, '1.5 KB'], [1024 ** 2, '1 MB'],
    [1024 ** 3, '1 GB'], [1024 ** 4, '1 TB'], [1024 ** 5, '1 PB'],
    [-1, '0 B'], [NaN, '0 B'], [Infinity, '0 B'], [-Infinity, '0 B'],
  ])('renders %s bytes as %s', (bytes, expected) => { expect(formatBytes(bytes)).toBe(expected); });

  it('supports empty string for non-positive values when configured', () => {
    expect(formatBytes(0, { emptyForNonPositive: true })).toBe('');
    expect(formatBytes(-10, { emptyForNonPositive: true })).toBe('');
    expect(formatBytes(1024, { emptyForNonPositive: true })).toBe('1 KB');
  });

  it('supports fixed decimals when configured', () => {
    expect(formatBytes(1024 * 1024 * 512, { fixedDecimals: true, decimals: 1 })).toBe('512.0 MB');
    expect(formatBytes(1024 * 1024 * 1024 * 2, { fixedDecimals: true, decimals: 1 })).toBe('2.0 GB');
    expect(formatBytes(100, { fixedDecimals: true, decimals: 1 })).toBe('100 B');
  });

  it('formatFileSize delegates to formatBytes', () => {
    expect(formatFileSize(0)).toBe('0 B');
    expect(formatFileSize(1024)).toBe('1 KB');
  });
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
  it('supports compact relative formatting', () => {
    const twoWeeksAgo = '2026-09-24T12:00:00Z';
    expect(formatRelativeTime(twoWeeksAgo, 'Recently', { compact: true })).toBe('2w ago');
  });
  it('formatRelativeDate returns null for nullish inputs', () => {
    expect(formatRelativeDate(null)).toBeNull();
    expect(formatRelativeDate(undefined)).toBeNull();
    expect(formatRelativeDate('2026-10-08T11:00:00Z')).toBe('1h ago');
  });
});

describe('formatTimeLeft', () => {
  it.each([
    [0, false, 'less than a minute'],
    [59, false, 'less than a minute'],
    [60, false, 'about 1 min'],
    [120, false, 'about 2 min'],
    [3600, false, 'about 1 h'],
    [4200, false, 'about 1 h 10 min'],
    [0, true, 'less than a minute'],
    [60, true, 'about 1 min left'],
    [4200, true, 'about 1 h 10 min left'],
    [7200, true, 'about 2 h left'],
  ])('formats %i seconds with suffix=%s as %s', (seconds, suffix, expected) => {
    expect(formatTimeLeft(seconds, { suffix })).toBe(expected);
  });
});

describe('formatEta', () => {
  it('formats approximate ETA', () => {
    expect(formatEta(0)).toBe('<1 min');
    expect(formatEta(59)).toBe('<1 min');
    expect(formatEta(180)).toBe('~3 min');
    expect(formatEta(3600)).toBe('~1 h');
    expect(formatEta(4200)).toBe('~1 h 10 min');
  });

  it('formats compact ETA', () => {
    expect(formatEta(null, 'compact')).toBe('—');
    expect(formatEta(-1, 'compact')).toBe('—');
    expect(formatEta(0, 'compact')).toBe('0s');
    expect(formatEta(59, 'compact')).toBe('59s');
    expect(formatEta(60, 'compact')).toBe('1m 0s');
    expect(formatEta(3661, 'compact')).toBe('1h 1m');
  });
});

describe('indexPercent and progressPercent', () => {
  it('calculates indexPercent with Math.floor', () => {
    expect(indexPercent(0, 100)).toBe(0);
    expect(indexPercent(50, 100)).toBe(50);
    expect(indexPercent(99.9, 100)).toBe(99);
    expect(indexPercent(150, 100)).toBe(100);
    expect(indexPercent(10, 0)).toBe(0);
  });

  it('calculates progressPercent with Math.round', () => {
    expect(progressPercent({ done: 0, total: 100 })).toBe(0);
    expect(progressPercent({ done: 50, total: 100 })).toBe(50);
    expect(progressPercent({ done: 50.6, total: 100 })).toBe(51);
    expect(progressPercent({ done: 150, total: 100 })).toBe(100);
    expect(progressPercent({ done: 10, total: 0 })).toBe(0);
    expect(progressPercent(50, 100)).toBe(50);
  });
});
