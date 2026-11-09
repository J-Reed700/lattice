import { describe, expect, it } from 'vitest';

import { HELP_MODES, helpWord } from './helpModes';

describe('helpWord', () => {
  it('names each mode in the learner words', () => {
    expect(helpWord('explore')).toBe('Full help');
    expect(helpWord('practice')).toBe('Hints only');
    expect(helpWord('demonstrate')).toBe('No help');
  });

  it('covers every mode the picker offers', () => {
    for (const mode of HELP_MODES) expect(helpWord(mode.id)).toBe(mode.help);
  });
});
