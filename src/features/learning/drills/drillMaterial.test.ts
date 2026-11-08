import { describe, expect, it } from 'vitest';

import type { CodeReviewDrillDto } from '@/lib/bindings';

import { kindOf, languageOf, materialLabel } from './drillMaterial';

const code: CodeReviewDrillDto = { title: 'A limiter', material: { kind: 'code', language: 'csharp' }, files: [] };
const passage: CodeReviewDrillDto = {
  title: 'Why spacing helps',
  material: { kind: 'passage', sourceTitle: 'Spacing and memory' },
  files: [],
};

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
