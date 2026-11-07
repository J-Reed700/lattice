import { describe, expect, it } from 'vitest';

import type { FitVerdictDto } from '@/lib/bindings';

import { fitLabel, fitUsage, formatGib } from './fitText';

const GB = 1_073_741_824;
const verdict = (kind: FitVerdictDto['kind'], needs = 11.2, total = 16): FitVerdictDto => ({
  kind,
  needsBytes: needs * GB,
  budgetBytes: 15 * GB,
  device: { name: 'NVIDIA GeForce RTX 4080', kind: 'dedicated', totalBytes: total * GB },
});

describe('fit wording', () => {
  it('words the backend verdicts', () => {
    expect(fitLabel(verdict('fits'))).toBe('Fits');
    expect(fitLabel(verdict('tight'))).toBe('Fits with a shorter context');
    expect(fitLabel(verdict('too_large'))).toBe('Too large for this computer');
  });

  it('says a size not yet read is being checked', () => {
    expect(fitLabel(null)).toBe('Checking size');
  });

  it('reports use against the device’s whole memory, not the verdict’s budget', () => {
    expect(fitUsage(verdict('fits'))).toBe('Uses 11.2 of 16 GB on NVIDIA GeForce RTX 4080');
  });

  it('writes whole gigabytes without a trailing .0 and others with one decimal', () => {
    expect(formatGib(8 * GB)).toBe('8');
    expect(formatGib(13.54 * GB)).toBe('13.5');
    expect(formatGib(0.96 * GB)).toBe('1');
  });
});
