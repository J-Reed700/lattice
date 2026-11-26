import { describe, expect, it } from 'vitest';

import { utf16ToByte } from './offsets';
import { selectorForLines } from './sourceAim';

describe('selectorForLines', () => {
  it('selects the aimed line without its trailing line break', () => {
    const source = 'Line zero\nLine one\nLine two\nLine three';
    const selector = selectorForLines(source, { lines: [2, 3], source, sourceStart: 0 });
    expect(selector).toEqual({
      exact: 'Line two',
      prefix: null,
      suffix: null,
      startByte: utf16ToByte(source, 19),
      endByte: utf16ToByte(source, 27),
    });
  });

  it('computes byte offsets relative to value when sourceStart > 0', () => {
    const prefix = 'Intro — 𝄞\n\n';
    const source = 'First line\nTarget line\nThird';
    const value = `${prefix}${source}\nOutro`;
    const sourceStart = prefix.length;
    const selector = selectorForLines(value, { lines: [1, 2], source, sourceStart });
    const start = sourceStart + 'First line\n'.length;
    const end = start + 'Target line'.length;
    expect(selector).toEqual({
      exact: 'Target line',
      prefix: null,
      suffix: null,
      startByte: utf16ToByte(value, start),
      endByte: utf16ToByte(value, end),
    });
  });

  it('returns null when value does not hold aim.source at sourceStart', () => {
    expect(selectorForLines('Other text', { lines: [0, 1], source: 'First', sourceStart: 0 })).toBeNull();
    expect(selectorForLines('First', { lines: [0, 1], source: 'First', sourceStart: 1 })).toBeNull();
  });

  it('drops trailing empty lines when the range spans multiple lines', () => {
    const source = 'First line\nSecond line\n\n\nAfter';
    const selector = selectorForLines(source, { lines: [0, 4], source, sourceStart: 0 });
    expect(selector).toEqual({
      exact: 'First line\nSecond line',
      prefix: null,
      suffix: null,
      startByte: 0,
      endByte: utf16ToByte(source, 'First line\nSecond line'.length),
    });
  });
});
