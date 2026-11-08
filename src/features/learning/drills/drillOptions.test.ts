import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_OPTIONS,
  DRILL_SIZES,
  languageLabel,
  materialOf,
  PASSAGE_SIZES,
  readChoices,
  readCourseLanguage,
  sizeOf,
  validMinutes,
  writeChoices,
  writeCourseLanguage,
} from './drillOptions';

beforeEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

const CODE = { material: 'code', language: null, lessonId: null } as const;

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
  it('are code, Practice, Standard and ten minutes, with no language and no lesson, before any are made', () => {
    expect(readChoices()).toEqual({ ...CODE, mode: 'practice', size: 'standard', minutes: 10 });
    expect(DEFAULT_OPTIONS).toEqual({ ...CODE, mode: 'practice', size: 'standard', minutes: 10 });
  });

  it('are the material, mode, size and minutes the learner started a drill with, and no language or lesson', () => {
    writeChoices({ material: 'passage', language: 'rust', lessonId: 'lesson-2', mode: 'interview', size: 'long', minutes: 25 });
    expect(readChoices()).toEqual({ material: 'passage', language: null, lessonId: null, mode: 'interview', size: 'long', minutes: 25 });
    expect(Object.keys(JSON.parse(localStorage.getItem('lattice.drills.choices') ?? '{}')).sort()).toEqual([
      'material',
      'minutes',
      'mode',
      'size',
    ]);
  });

  it('keep ten minutes when the limit typed was not a time', () => {
    writeChoices({ ...DEFAULT_OPTIONS, mode: 'interview', size: 'short', minutes: Number.NaN });
    expect(readChoices()).toEqual({ ...CODE, mode: 'interview', size: 'short', minutes: 10 });
  });

  it('do not read back a language, a course or a topic that an older version kept', () => {
    localStorage.setItem(
      'lattice.drills.choices',
      JSON.stringify({ material: 'passage', language: 'sql', programId: 'course-1', topic: 'spacing', mode: 'interview', size: 'long', minutes: 15 }),
    );
    expect(readChoices()).toEqual({ material: 'passage', language: null, lessonId: null, mode: 'interview', size: 'long', minutes: 15 });
  });

  it('are the defaults where what was kept cannot be read, and field by field where only some is wrong', () => {
    localStorage.setItem('lattice.drills.choices', '{not json');
    expect(readChoices()).toEqual(DEFAULT_OPTIONS);
    localStorage.setItem('lattice.drills.choices', 'null');
    expect(readChoices()).toEqual(DEFAULT_OPTIONS);
    localStorage.setItem('lattice.drills.choices', JSON.stringify({ material: 'poetry', mode: 'rapid', size: 'long', minutes: -1 }));
    expect(readChoices()).toEqual({ ...CODE, mode: 'practice', size: 'long', minutes: 10 });
  });

  it('are the defaults where the browser keeps nothing, and are lost quietly where it cannot be written to', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(readChoices()).toEqual(DEFAULT_OPTIONS);
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota');
    });
    expect(() => writeChoices(DEFAULT_OPTIONS)).not.toThrow();
  });
});

describe('the language of a course', () => {
  it('is none before one is written, and the one written after', () => {
    expect(readCourseLanguage('course-1')).toBeNull();
    writeCourseLanguage('course-1', 'rust');
    expect(readCourseLanguage('course-1')).toBe('rust');
  });

  it('is read back for its course and not for another', () => {
    writeCourseLanguage('course-1', 'rust');
    expect(readCourseLanguage('course-1')).toBe('rust');
    expect(readCourseLanguage('course-2')).toBeNull();
    writeCourseLanguage('course-2', 'sql');
    expect(readCourseLanguage('course-1')).toBe('rust');
    expect(readCourseLanguage('course-2')).toBe('sql');
    expect(localStorage.getItem('lattice.drills.language.course-1')).toBe('rust');
  });

  it('is none where what was kept is empty or cannot be read, and is lost quietly where it cannot be written', () => {
    localStorage.setItem('lattice.drills.language.course-1', 'go');
    expect(readCourseLanguage('course-1')).toBe('go');
    localStorage.setItem('lattice.drills.language.course-1', '');
    expect(readCourseLanguage('course-1')).toBeNull();
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(readCourseLanguage('course-1')).toBeNull();
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota');
    });
    expect(() => writeCourseLanguage('course-1', 'go')).not.toThrow();
  });
});

describe('what is asked to review', () => {
  it('is code in the language chosen', () => {
    expect(materialOf({ ...DEFAULT_OPTIONS, language: 'rust' })).toEqual({ kind: 'code', language: 'rust' });
  });

  it('is a passage, which names no language', () => {
    expect(materialOf({ ...DEFAULT_OPTIONS, material: 'passage', language: 'rust' })).toEqual({ kind: 'passage' });
    expect(materialOf({ ...DEFAULT_OPTIONS, material: 'passage' })).toEqual({ kind: 'passage' });
  });

  it('is nothing for code before a language is chosen', () => {
    expect(materialOf(DEFAULT_OPTIONS)).toBeNull();
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
