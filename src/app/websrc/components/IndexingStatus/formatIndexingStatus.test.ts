import { describe, it, expect } from 'vitest';

import type { IndexingSnapshot } from '@/types/api/indexing';

import {
  formatFailureLine,
  formatIndexingAriaLabel,
  formatIndexingLine,
  shouldShowIndexingRail,
  shouldSpin,
} from './formatIndexingStatus';


function snapshot(overrides: Partial<IndexingSnapshot> = {}): IndexingSnapshot {
  return {
    totalFiles: 0,
    processed: 0,
    failed: 0,
    status: 'idle',
    percentage: 0,
    paused: false,
    failures: [],
    ...overrides,
  };
}

describe('formatIndexingLine', () => {
  it('says Scanning… while scanning', () => {
    expect(formatIndexingLine(snapshot({ status: 'scanning' }))).toBe('Scanning…');
  });

  it('counts files while processing', () => {
    expect(
      formatIndexingLine(snapshot({ status: 'processing', processed: 12, totalFiles: 47 })),
    ).toBe('Indexing 12 of 47');
  });

  it('drops the count when the total is not known yet', () => {
    expect(formatIndexingLine(snapshot({ status: 'processing', totalFiles: 0 }))).toBe('Indexing…');
  });

  it('names the pause', () => {
    expect(
      formatIndexingLine(
        snapshot({ status: 'processing', paused: true, processed: 12, totalFiles: 47 }),
      ),
    ).toBe('Paused · 12 of 47');
  });

  it('reports a clean completion', () => {
    expect(
      formatIndexingLine(snapshot({ status: 'complete', processed: 47, totalFiles: 47 })),
    ).toBe('Indexed 47 files');
  });

  it('reports a completion with failures', () => {
    expect(
      formatIndexingLine(
        snapshot({ status: 'complete', processed: 47, totalFiles: 47, failed: 3 }),
      ),
    ).toBe('Indexed 44 of 47 files');
  });

  it('reports where a cancelled run stopped', () => {
    expect(
      formatIndexingLine(snapshot({ status: 'cancelled', processed: 12, totalFiles: 47 })),
    ).toBe('Stopped at 12 of 47');
  });

  it('reports an errored run', () => {
    expect(formatIndexingLine(snapshot({ status: 'error' }))).toBe('Indexing stopped');
  });

  it('says nothing when idle', () => {
    expect(formatIndexingLine(snapshot({ status: 'idle' }))).toBeNull();
    expect(formatIndexingLine(undefined)).toBeNull();
  });

  it('groups thousands', () => {
    expect(
      formatIndexingLine(snapshot({ status: 'processing', processed: 1204, totalFiles: 12000 })),
    ).toBe('Indexing 1,204 of 12,000');
  });
});

describe('formatFailureLine', () => {
  it('is singular for one failure', () => {
    expect(formatFailureLine(snapshot({ failed: 1 }))).toBe('1 failed');
  });

  it('is the same word for many', () => {
    expect(formatFailureLine(snapshot({ failed: 3 }))).toBe('3 failed');
  });

  it('is null when nothing failed', () => {
    expect(formatFailureLine(snapshot())).toBeNull();
  });
});

describe('formatIndexingAriaLabel', () => {
  it('joins the run line and the failure line', () => {
    expect(
      formatIndexingAriaLabel(
        snapshot({ status: 'processing', processed: 12, totalFiles: 47, failed: 3 }),
      ),
    ).toBe('Indexing 12 of 47, 3 failed');
  });

  it('falls back to the failure line alone', () => {
    expect(formatIndexingAriaLabel(snapshot({ status: 'idle', failed: 2 }))).toBe('2 failed');
  });

  it('has a neutral fallback', () => {
    expect(formatIndexingAriaLabel(snapshot())).toBe('Indexing status');
  });
});

describe('shouldShowIndexingRail', () => {
  const failure = {
    path: '/vault/a.pdf',
    fileName: 'a.pdf',
    reason: 'boom',
    failedAt: '2026-09-06T12:00:00Z',
  };

  it('hides on an idle vault with no failures', () => {
    expect(shouldShowIndexingRail(snapshot())).toBe(false);
    expect(shouldShowIndexingRail(undefined)).toBe(false);
  });

  it('shows while running', () => {
    expect(shouldShowIndexingRail(snapshot({ status: 'processing' }))).toBe(true);
  });

  it('shows while paused', () => {
    expect(shouldShowIndexingRail(snapshot({ status: 'processing', paused: true }))).toBe(true);
  });

  it('shows when only failures remain', () => {
    expect(shouldShowIndexingRail(snapshot({ status: 'complete', failures: [failure] }))).toBe(
      true,
    );
  });
});

describe('shouldSpin', () => {
  it('spins only while a run is genuinely in flight', () => {
    expect(shouldSpin(snapshot({ status: 'processing' }))).toBe(true);
    expect(shouldSpin(snapshot({ status: 'processing', paused: true }))).toBe(false);
    expect(shouldSpin(snapshot({ status: 'complete' }))).toBe(false);
  });
});
