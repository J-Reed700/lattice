import { expect, it } from 'vitest';

import { basename, formatEta, formatProgressBytes, formatSpeed, isActive, normalizePercentage } from './downloadFormat';

it.each([[null, '—'], [-1, '—'], [NaN, '—'], [Infinity, '—'], [0, '0s'], [59.49, '59s'], [59.5, '1m 0s'], [3599, '59m 59s'], [3600, '1h 0m'], [3661, '1h 1m']] as const)('formats ETA %s without invalid time units', (seconds, expected) => {
  expect(formatEta(seconds)).toBe(expected);
});
it.each([0, -1, NaN, Infinity])('shows unknown speed for %s', speed => { expect(formatSpeed(speed)).toBe('—'); });
it('formats a stalled download with an unknown total', () => {
  expect(formatSpeed(1024)).toBe('1 KB/s');
  expect(formatProgressBytes(512, null)).toBe('512 B');
  expect(formatProgressBytes(512, 0)).toBe('512 B');
  expect(formatProgressBytes(512, 1024)).toBe('512 B / 1 KB');
});
it.each([
  [null, 0, null, null], [null, 10, 0, null], [NaN, 10, 20, 50],
  [Infinity, 5, 20, 25], [null, -1, 20, 0], [null, 40, 20, 100],
  [-1, 0, null, 0], [110, 0, null, 100], [0, 15, 20, 0],
  [null, NaN, 20, null], [null, 10, Infinity, null],
] as const)('normalizes percentage %s, downloaded %s, total %s', (percentage, downloaded, total, expected) => {
  expect(normalizePercentage(percentage, downloaded, total)).toBe(expected);
});
it.each([['Pending', true], ['Downloading', true], ['Paused', true], ['Completed', false], ['Failed', false], ['Cancelled', false]] as const)('classifies %s activity', (state, active) => { expect(isActive(state)).toBe(active); });
it.each([['C:\\Downloads\\café.gguf', 'café.gguf'], ['/downloads/model.gguf', 'model.gguf'], ['model.gguf', 'model.gguf'], ['folder/', 'folder/'], ['', '']])('labels the destination %s', (path, expected) => { expect(basename(path)).toBe(expected); });
