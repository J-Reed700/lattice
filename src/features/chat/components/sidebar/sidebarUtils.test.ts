import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  formatJournalGroupLabel,
  formatRoleLabel,
  formatShortRelativeTime,
  getLocalDayKey,
  getTimeBucket,
  isReferenceInboxEnabled,
} from './sidebarUtils';

describe('formatRoleLabel', () => {
  it.each([
    ['assistant', 'Assistant'],
    ['USER', 'You'],
    ['system', 'System'],
    ['tool', 'Tool'],
    ['', ''],
    [null, ''],
    [undefined, ''],
  ])('labels %s as %s', (role, label) => {
    expect(formatRoleLabel(role)).toBe(label);
  });
});

describe('time formatting', () => {
  const now = new Date(2026, 4, 15, 12, 0, 0);

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('shortens relative times to a number and a unit', () => {
    expect(formatShortRelativeTime(new Date(2026, 4, 15, 11, 48).toISOString())).toBe('12m');
    expect(formatShortRelativeTime(new Date(2026, 4, 13, 12).toISOString())).toBe('2d');
    expect(formatShortRelativeTime(new Date(2026, 4, 15, 9).toISOString())).toBe('3h');
    expect(formatShortRelativeTime(new Date(2026, 1, 15, 12).toISOString())).toBe('3mo');
    expect(formatShortRelativeTime(new Date(2024, 4, 15, 12).toISOString())).toBe('2y');
  });

  it('returns nothing for missing or unreadable times', () => {
    expect(formatShortRelativeTime(null)).toBe('');
    expect(formatShortRelativeTime(undefined)).toBe('');
    expect(formatShortRelativeTime('not a date')).toBe('');
  });

  it('keys a day by its local calendar date', () => {
    expect(getLocalDayKey(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05');
  });

  it('names today and yesterday, and dates further back', () => {
    expect(formatJournalGroupLabel(new Date(2026, 4, 15, 6))).toBe('Today');
    expect(formatJournalGroupLabel(new Date(2026, 4, 14, 22))).toBe('Yesterday');
    const thisYear = formatJournalGroupLabel(new Date(2026, 4, 1));
    expect(thisYear).not.toMatch(/2026/);
    expect(thisYear).toMatch(/1/);
    expect(formatJournalGroupLabel(new Date(2025, 4, 1))).toMatch(/2025/);
  });

  it.each([
    [new Date(2026, 4, 15, 1), 'today'],
    [new Date(2026, 4, 14, 1), 'yesterday'],
    [new Date(2026, 4, 10), 'last-7-days'],
    [new Date(2026, 3, 25), 'last-30-days'],
    [new Date(2026, 0, 1), 'older'],
  ] as const)('puts %s in the %s bucket', (date, bucket) => {
    expect(getTimeBucket(date, now)).toBe(bucket);
  });
});

describe('isReferenceInboxEnabled', () => {
  afterEach(() => {
    localStorage.removeItem('feature.referenceInbox.v1');
    vi.restoreAllMocks();
  });

  it('is on unless switched off', () => {
    expect(isReferenceInboxEnabled()).toBe(true);
    localStorage.setItem('feature.referenceInbox.v1', 'true');
    expect(isReferenceInboxEnabled()).toBe(true);
    localStorage.setItem('feature.referenceInbox.v1', 'false');
    expect(isReferenceInboxEnabled()).toBe(false);
  });

  it('stays on when storage cannot be read', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(isReferenceInboxEnabled()).toBe(true);
  });
});
