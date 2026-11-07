import { beforeEach, describe, expect, it } from 'vitest';

import {
  DEFAULT_OPTIONS,
  DRILL_SIZES,
  languageLabel,
  materialOf,
  PASSAGE_SIZES,
  readChoices,
  sizeOf,
  validMinutes,
  writeChoices,
} from './drillOptions';

beforeEach(() => localStorage.clear());

const CODE = { material: 'code', programId: null, topic: '' } as const;

describe('the sizes of a drill', () => {
  it('are Short, Standard and Long, in the sizes the backend builds', () => {
    expect(DRILL_SIZES.map(({ id, label, lines, issues }) => [id, label, lines, issues])).toEqual([
      ['short', 'Short', 40, 3],
      ['standard', 'Standard', 80, 4],
      ['long', 'Long', 140, 5],
    ]);
  });

  it('ask for some code and for something to plant, and are in the 20 to 150 lines a drill has', () => {
    for (const size of DRILL_SIZES) {
      expect(size.lines).toBeGreaterThanOrEqual(20);
      expect(size.lines).toBeLessThanOrEqual(150);
      expect(size.issues).toBeGreaterThanOrEqual(1);
    }
  });

  it('are found by id, and an id that is none is the Standard size', () => {
    expect(sizeOf('long').lines).toBe(140);
    expect(sizeOf('huge' as never).id).toBe('standard');
  });
});

describe('a language on screen', () => {
  it('is named as the code views name it', () => {
    expect(languageLabel('csharp')).toBe('C#');
    expect(languageLabel('cpp')).toBe('C++');
    expect(languageLabel('python')).toBe('Python');
  });

  it('is its id when no view knows it', () => {
    expect(languageLabel('zig')).toBe('zig');
  });
});

describe('a time limit', () => {
  it('is a whole number of minutes, at least one', () => {
    expect([10, 1, 90].map(validMinutes)).toEqual([true, true, true]);
    expect([0, -5, 2.5, Number.NaN].map(validMinutes)).toEqual([false, false, false, false]);
  });
});

describe('the last choices', () => {
  it('are Python, Practice, Standard and ten minutes before any are made', () => {
    expect(readChoices()).toEqual({ ...CODE, language: 'python', mode: 'practice', size: 'standard', minutes: 10 });
    expect(DEFAULT_OPTIONS.language).toBe('python');
  });

  it('are the ones the learner started a drill with', () => {
    writeChoices({ ...CODE, language: 'rust', mode: 'interview', size: 'long', minutes: 25 });
    expect(readChoices()).toEqual({ ...CODE, language: 'rust', mode: 'interview', size: 'long', minutes: 25 });
  });

  it('keep ten minutes when the limit typed was not a time', () => {
    writeChoices({ ...CODE, language: 'go', mode: 'interview', size: 'short', minutes: Number.NaN });
    expect(readChoices()).toEqual({ ...CODE, language: 'go', mode: 'interview', size: 'short', minutes: 10 });
  });

  it('are the defaults where what was kept cannot be read, and field by field where only some is wrong', () => {
    localStorage.setItem('lattice.drills.choices', '{not json');
    expect(readChoices()).toEqual(DEFAULT_OPTIONS);
    localStorage.setItem('lattice.drills.choices', JSON.stringify({ language: 'sql', mode: 'rapid', size: 'long', minutes: -1 }));
    expect(readChoices()).toEqual({ ...CODE, language: 'sql', mode: 'practice', size: 'long', minutes: 10 });
  });

  it('keep a passage from a course and its topic like the language', () => {
    const passage = { ...DEFAULT_OPTIONS, material: 'passage' as const, programId: 'course-1', topic: 'spacing' };
    writeChoices(passage);
    expect(readChoices()).toEqual(passage);
    localStorage.setItem('lattice.drills.choices', JSON.stringify({ material: 'poetry', programId: 7, topic: 3 }));
    expect(readChoices()).toEqual(DEFAULT_OPTIONS);
  });
});

describe('what is asked to review', () => {
  it('is code in the chosen language', () => {
    expect(materialOf({ ...DEFAULT_OPTIONS, language: 'rust' })).toEqual({ kind: 'code', language: 'rust' });
  });

  it('is a passage of the chosen course, with the topic when one was typed', () => {
    const passage = { ...DEFAULT_OPTIONS, material: 'passage' as const, programId: 'course-1' };
    expect(materialOf(passage)).toEqual({ kind: 'passage', programId: 'course-1', topic: null });
    expect(materialOf({ ...passage, topic: '  spacing  ' })).toEqual({ kind: 'passage', programId: 'course-1', topic: 'spacing' });
  });

  it('is nothing for a passage before a course is chosen', () => {
    expect(materialOf({ ...DEFAULT_OPTIONS, material: 'passage' })).toBeNull();
  });
});

describe('the sizes of a drill on a passage', () => {
  it('are sentences to explain and errors to plant, enough sentences for every error', () => {
    expect(PASSAGE_SIZES.map(({ id, lines, issues }) => [id, lines, issues])).toEqual([
      ['short', 6, 2],
      ['standard', 10, 3],
      ['long', 14, 4],
    ]);
    for (const size of PASSAGE_SIZES) expect(size.lines).toBeGreaterThanOrEqual(2 * size.issues - 1);
    expect(sizeOf('long', 'passage').lines).toBe(14);
    expect(sizeOf('long').lines).toBe(140);
  });
});
