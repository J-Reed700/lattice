import { describe, expect, it } from 'vitest';

import type { CodeReviewDrillDto, DrillLessonRefDto } from '@/lib/bindings';

import { drillMeta, kindOf, languageOf, materialLabel } from './drillMaterial';

const code: CodeReviewDrillDto = { title: 'A limiter', material: { kind: 'code', language: 'csharp' }, files: [], lesson: null, issues: 4 };
const passage: CodeReviewDrillDto = {
  title: 'Why spacing helps',
  material: { kind: 'passage', sourceTitle: 'Spacing and memory' },
  files: [],
  lesson: null,
  issues: 2,
};
const lesson: DrillLessonRefDto = { programId: 'course-1', lessonId: 'lesson-2', title: 'Spacing' };

describe('what a drill is made of', () => {
  it('is code in a language, named as the code views name it', () => {
    expect(kindOf(code)).toBe('code');
    expect(languageOf(code)).toBe('csharp');
    expect(materialLabel(code)).toBe('C#');
  });

  it('is an explanation of a source for a passage, which has no language', () => {
    expect(kindOf(passage)).toBe('passage');
    expect(languageOf(passage)).toBeNull();
    expect(materialLabel(passage)).toBe('Explanation of Spacing and memory');
  });
});

describe('the line that says what a drill is', () => {
  it('names the lesson, the language and the mode of a drill on code', () => {
    expect(drillMeta({ ...code, lesson }, 'interview')).toBe('Spacing · C# · Timed');
  });

  it('names the lesson, the source and the mode of a drill on a passage', () => {
    expect(drillMeta({ ...passage, lesson }, 'practice')).toBe('Spacing · Explanation of Spacing and memory · Practice');
  });

  it('leaves the lesson out of a drill that names none', () => {
    expect(drillMeta(code, 'practice')).toBe('C# · Practice');
  });
});
