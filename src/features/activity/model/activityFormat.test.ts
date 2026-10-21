import { describe, expect, it } from 'vitest';

import type { ActivityEntryDto } from '@/lib/bindings';

import {
  activityLabel,
  activitySummary,
  formatProgress,
  formatRetry,
  formatTimeLeft,
  progressPercent,
  stateLine,
} from './activityFormat';

function entry(overrides: Partial<ActivityEntryDto> = {}): ActivityEntryDto {
  return {
    id: 'job-1',
    kind: 'learning.course_index',
    title: 'Indexing evidence for Calculus',
    state: 'running',
    detail: null,
    progress: null,
    perSecond: null,
    secondsLeft: null,
    holding: null,
    target: null,
    since: null,
    retryAt: null,
    ...overrides,
  };
}

describe('formatTimeLeft', () => {
  it('reads as people say it', () => {
    expect(formatTimeLeft(0)).toBe('less than a minute');
    expect(formatTimeLeft(59)).toBe('less than a minute');
    expect(formatTimeLeft(60)).toBe('about 1 min left');
    expect(formatTimeLeft(12 * 60)).toBe('about 12 min left');
    expect(formatTimeLeft(59 * 60 + 40)).toBe('about 1 h left');
    expect(formatTimeLeft(80 * 60)).toBe('about 1 h 20 min left');
    expect(formatTimeLeft(3 * 3600)).toBe('about 3 h left');
  });
});

describe('formatRetry', () => {
  it('says when a deferred job tries again', () => {
    expect(formatRetry(10_000, 0)).toBe('Tries again in less than a minute');
    expect(formatRetry(3 * 60_000, 0)).toBe('Tries again in about 3 min');
    expect(formatRetry(0, 5_000)).toBe('Tries again in less than a minute');
  });
});

describe('formatProgress', () => {
  it('counts in the unit the work counts in', () => {
    expect(formatProgress({ done: 120, total: 900, unit: 'passages' })).toBe('120 of 900 passages');
    expect(formatProgress({ done: 1024 * 1024 * 512, total: 1024 * 1024 * 1024 * 2, unit: 'bytes' })).toBe('512.0 MB of 2.0 GB');
    expect(progressPercent({ done: 1, total: 3, unit: 'files' })).toBe(33);
    expect(progressPercent({ done: 0, total: 0, unit: 'files' })).toBe(0);
  });
});

describe('stateLine', () => {
  it('says what an item does, or a plain word for its state', () => {
    expect(stateLine(entry({ detail: 'Embedding passages: 120 of 900' }), 0)).toBe('Embedding passages: 120 of 900');
    expect(stateLine(entry({ state: 'waiting' }), 0)).toBe('Waiting');
    expect(stateLine(entry({ state: 'paused', detail: 'Paused while Calculus embeds first' }), 0)).toBe(
      'Paused while Calculus embeds first',
    );
  });

  it('adds when a job put off by an outage tries again', () => {
    const deferred = entry({ state: 'waiting', detail: 'Connection interrupted', retryAt: 5 * 60_000 });
    expect(stateLine(deferred, 0)).toBe('Connection interrupted. Tries again in about 5 min');
    // Once the time has passed it only says why it waited.
    expect(stateLine(deferred, 6 * 60_000)).toBe('Connection interrupted');
  });
});

describe('labels', () => {
  it('count what runs', () => {
    expect(activityLabel(3, 5)).toBe('Activity: 3 running');
    expect(activityLabel(0, 2)).toBe('Activity: 2 waiting or paused');
    expect(activityLabel(0, 0)).toBe('Activity: nothing running');
    expect(activitySummary([])).toBe('Nothing is running.');
    expect(
      activitySummary([entry(), entry({ id: 'b' }), entry({ id: 'c', state: 'paused' }), entry({ id: 'd', state: 'waiting' })]),
    ).toBe('2 running, 1 paused, 1 waiting');
  });
});
