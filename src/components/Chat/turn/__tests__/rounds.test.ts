import { describe, expect, it } from 'vitest';

import type { TurnStep } from '@/types/conversation';

import { roundCount, roundOf, sectionsOf } from '../rounds';

const step = (id: string, kind: TurnStep['kind']): TurnStep => ({
  id,
  kind,
  label: id,
  state: 'done',
  startedAtMs: 0,
});

describe('rounds', () => {
  it('keeps a turn that looked once as one unnumbered section', () => {
    const steps = [step('route', 'route'), step('search', 'web_search'), step('write', 'generate')];
    expect(sectionsOf(steps)).toEqual([{ round: null, steps }]);
    expect(roundCount(steps)).toBe(0);
  });

  it('starts a new round at each generation that was followed by more looking', () => {
    const steps = [
      step('route', 'route'),
      step('search', 'web_search'),
      step('page-1', 'read_page'),
      step('think-1', 'generate'),
      step('page-2', 'read_page'),
      step('think-2', 'generate'),
      step('page-3', 'read_page'),
      step('write', 'generate'),
      step('check', 'verify'),
    ];
    expect(sectionsOf(steps).map(section => [section.round, section.steps.map(s => s.id)])).toEqual([
      [null, ['route']],
      [1, ['search', 'page-1']],
      [null, ['think-1']],
      [2, ['page-2']],
      [null, ['think-2']],
      [3, ['page-3']],
      [null, ['write', 'check']],
    ]);
    expect(roundOf(steps, 'page-3')).toBe(3);
    expect(roundOf(steps, 'write')).toBeNull();
  });

  /** A provider retry happens inside a generation; it is not a trip anywhere. */
  it('does not count a retry between two generations as a round', () => {
    const steps = [
      step('search', 'web_search'),
      step('think', 'generate'),
      step('again', 'retry'),
      step('write', 'generate'),
    ];
    expect(roundCount(steps)).toBe(0);
  });
});
