import { describe, expect, it } from 'vitest';

import { readTrigger, replaceTrigger } from '../suggestTrigger';

describe('the composer trigger', () => {
  it('opens the moment a slash starts a word', () => {
    expect(readTrigger('/', 1)).toEqual({ kind: 'slash', query: '', start: 0, end: 1 });
    expect(readTrigger('ask me /de', 10)).toEqual({
      kind: 'slash',
      query: 'de',
      start: 7,
      end: 10,
    });
  });

  it('leaves a slash inside a word alone', () => {
    // "and/or" is prose, not a command.
    expect(readTrigger('and/or', 6)).toBeNull();
  });

  it('closes once the token takes a space', () => {
    expect(readTrigger('/deep learning', 14)).toBeNull();
  });

  it('opens on an at-sign and reads the file name being typed', () => {
    expect(readTrigger('Compare @halv', 13)).toEqual({
      kind: 'mention',
      query: 'halv',
      start: 8,
      end: 13,
    });
  });

  it('answers for the caret, not for the end of the text', () => {
    // The caret sits after "/de"; the rest of the sentence is behind it.
    expect(readTrigger('/de and more', 3)).toEqual({
      kind: 'slash',
      query: 'de',
      start: 0,
      end: 3,
    });
  });

  it('says nothing while text is selected', () => {
    expect(readTrigger('/deep', 1, 5)).toBeNull();
  });

  it('takes the typed token back out and reports where the caret lands', () => {
    const trigger = readTrigger('ask me /deep', 12);
    expect(trigger).not.toBeNull();
    expect(replaceTrigger('ask me /deep', trigger!, '')).toEqual({
      value: 'ask me ',
      caret: 7,
    });
  });
});
