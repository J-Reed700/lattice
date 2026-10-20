import { describe, expect, it } from 'vitest';

import {
  describeIndexCounts,
  describeIndexWait,
  describeTimeLeft,
  formatTimeLeft,
  indexPercent,
  secondsLeft,
  type IndexingStatus,
} from '@/features/learning/sources/courseIndexProgress';

const indexing = (overrides: Partial<IndexingStatus> = {}): IndexingStatus =>
  ({ state: 'indexing', embedded: 4_210, total: 13_457, referencesNew: 0, passagesPerSecond: 14, waitingTurn: false, ...overrides });

describe('Course index progress in words', () => {
  it.each([
    [0, 'less than a minute'],
    [59.9, 'less than a minute'],
    [60, 'about 1 min'],
    [660.5, 'about 11 min'],
    [3_569, 'about 59 min'],
    [3_570, 'about 1 h'],
    [4_200, 'about 1 h 10 min'],
    [7_200, 'about 2 h'],
    [9_000, 'about 2 h 30 min'],
  ])('says %s seconds left as "%s"', (seconds, words) => {
    expect(formatTimeLeft(seconds)).toBe(words);
  });

  it('computes the time left from the measured pace, and never guesses without one', () => {
    expect(secondsLeft(indexing())).toBeCloseTo(9_247 / 14);
    expect(secondsLeft(indexing({ passagesPerSecond: null }))).toBeNull();
    expect(secondsLeft(indexing({ passagesPerSecond: 0 }))).toBeNull();
    expect(secondsLeft(indexing({ embedded: 0, total: 0 }))).toBeNull();
    expect(secondsLeft(indexing({ embedded: 13_457 }))).toBe(0);
  });

  it('says the time left, that it is measuring speed, or nothing while it waits its turn', () => {
    expect(describeTimeLeft(indexing())).toBe('about 11 min left');
    expect(describeTimeLeft(indexing({ embedded: 13_200 }))).toBe('less than a minute left');
    expect(describeTimeLeft(indexing({ passagesPerSecond: null }))).toBe('measuring speed…');
    expect(describeTimeLeft(indexing({ waitingTurn: true, passagesPerSecond: null }))).toBeNull();
  });

  it('counts passages, or the new references before any passage is counted', () => {
    expect(describeIndexCounts(indexing())).toBe('4,210 of 13,457 passages');
    expect(describeIndexCounts(indexing({ embedded: 0, total: 0, referencesNew: 402 }))).toBe('Adding 402 new references');
    expect(describeIndexCounts(indexing({ embedded: 0, total: 0, referencesNew: 1 }))).toBe('Adding 1 new reference');
    expect(describeIndexCounts(indexing({ embedded: 0, total: 0 }))).toBeNull();
  });

  it('fills the bar by whole percent, never full before the last passage', () => {
    expect(indexPercent(4_210, 13_457)).toBe(31);
    expect(indexPercent(13_456, 13_457)).toBe(99);
    expect(indexPercent(13_457, 13_457)).toBe(100);
    expect(indexPercent(0, 0)).toBe(0);
  });

  it('gives each held lesson action its reason with the time left, and none once the course is not indexing', () => {
    expect(describeIndexWait(indexing())).toBe('Waits for the course index · about 11 min left');
    expect(describeIndexWait(indexing({ passagesPerSecond: null }))).toBe('Waits for the course index · measuring speed…');
    expect(describeIndexWait(indexing({ embedded: 0, total: 0, referencesNew: 3, passagesPerSecond: null }))).toBe('Waits for the course index');
    expect(describeIndexWait(indexing({ waitingTurn: true, passagesPerSecond: null }))).toBe('Waits for the course index · queued behind another course');
    expect(describeIndexWait({ state: 'ready', mode: 'hybrid' })).toBeUndefined();
    expect(describeIndexWait({ state: 'noEmbeddingModel' })).toBeUndefined();
    expect(describeIndexWait({ state: 'unavailable', reason: 'Damaged.' })).toBeUndefined();
    expect(describeIndexWait(null)).toBeUndefined();
  });
});
