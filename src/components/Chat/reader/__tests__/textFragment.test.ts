import { describe, expect, it } from 'vitest';

import { textFragmentUrl } from '../textFragment';

const PASSAGE =
  'Satellite surface temperature overstates the gap residents feel, and air temperature differences between the leafiest and barest blocks were closer to three degrees.';

describe('pointing a browser at the passage', () => {
  it('anchors a long passage on its first and last few words', () => {
    expect(textFragmentUrl('https://example.com/piece', PASSAGE)).toBe(
      'https://example.com/piece#:~:text=Satellite%20surface%20temperature%20overstates%20the' +
        ',were%20closer%20to%20three%20degrees.'
    );
  });

  it('quotes a short passage whole', () => {
    expect(textFragmentUrl('https://example.com/piece', 'Air temperature differences were small.')).toBe(
      'https://example.com/piece#:~:text=Air%20temperature%20differences%20were%20small.'
    );
  });

  it('escapes the hyphen, which is the directive\'s own syntax', () => {
    expect(textFragmentUrl('https://example.com/p', 'block-group census data')).toBe(
      'https://example.com/p#:~:text=block%2Dgroup%20census%20data'
    );
  });

  it('replaces a fragment the URL already carries', () => {
    expect(textFragmentUrl('https://example.com/p#section-2', 'block-group census data')).toBe(
      'https://example.com/p#:~:text=block%2Dgroup%20census%20data'
    );
  });

  it('leaves the URL alone when there is no passage to point at', () => {
    expect(textFragmentUrl('https://example.com/p#section-2', '   ')).toBe(
      'https://example.com/p#section-2'
    );
  });
});
