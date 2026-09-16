import { beforeEach, describe, expect, it } from 'vitest';

import {
  approximateScrollRatio,
  locatorFromSource,
  buildNeedles,
  forgetAllLocations,
  formatSourceLocation,
  isTimestampSection,
  normalizeForMatch,
  recallLocation,
  rememberLocation,
  timestampSectionStartSeconds,
} from '../passageLocator';

describe('normalizeForMatch', () => {
  it('collapses whitespace and folds case', () => {
    expect(normalizeForMatch('  The   QUICK\n brown  ')).toBe('the quick brown');
  });

  it('strips punctuation that markdown and OCR mangle', () => {
    expect(normalizeForMatch('Hello, world! (again) — really.')).toBe(
      'hello world again really'
    );
  });

  it('normalizes smart quotes and dashes to their plain forms', () => {
    expect(normalizeForMatch('“don’t”')).toBe('"don\'t"');
  });

  it('returns an empty string for punctuation-only input', () => {
    expect(normalizeForMatch('!!! ??? ...')).toBe('');
  });
});

describe('buildNeedles', () => {
  it('emits progressively shorter needles, longest first', () => {
    const text = 'word '.repeat(80);
    const needles = buildNeedles(text);
    expect(needles).toHaveLength(3);
    expect(needles[0].length).toBe(160);
    expect(needles[1].length).toBe(80);
    expect(needles[2].length).toBe(40);
    expect(needles[0].startsWith(needles[1])).toBe(true);
    expect(needles[1].startsWith(needles[2])).toBe(true);
  });

  it('does not repeat a needle when the text is shorter than a tier', () => {
    const needles = buildNeedles('a short but usable passage of text');
    expect(new Set(needles).size).toBe(needles.length);
  });

  it('returns nothing for text too short to identify a passage', () => {
    expect(buildNeedles('hi')).toEqual([]);
    expect(buildNeedles('   ')).toEqual([]);
  });
});

describe('formatSourceLocation', () => {
  beforeEach(() => {
    forgetAllLocations();
  });

  it('prefers a resolved page', () => {
    expect(formatSourceLocation({ section: 'Methods', chunkId: 'c1' }, 12)).toBe('p. 12');
  });

  it('renders a heading section with a section mark', () => {
    expect(formatSourceLocation({ section: 'Methods', chunkId: 'c1' })).toBe('§ Methods');
  });

  it('renders a timestamp section verbatim', () => {
    expect(formatSourceLocation({ section: '12:40–13:05', chunkId: 'c1' })).toBe('12:40–13:05');
  });

  it('returns null rather than inventing a location', () => {
    expect(formatSourceLocation({ section: undefined, chunkId: 'c1' })).toBeNull();
  });

  it('falls back to a remembered location for the chunk', () => {
    rememberLocation('c1', 'p. 7');
    expect(formatSourceLocation({ section: undefined, chunkId: 'c1' })).toBe('p. 7');
  });
});

describe('isTimestampSection and timestampSectionStartSeconds', () => {
  it('recognises a mm:ss range', () => {
    expect(isTimestampSection('12:40–13:05')).toBe(true);
    expect(timestampSectionStartSeconds('12:40–13:05')).toBe(760);
  });

  it('recognises an h:mm:ss range', () => {
    expect(timestampSectionStartSeconds('1:02:30–1:03:00')).toBe(3750);
  });

  it('rejects a heading', () => {
    expect(isTimestampSection('Methods')).toBe(false);
    expect(timestampSectionStartSeconds('Methods')).toBeNull();
  });

  it('handles an absent section', () => {
    expect(isTimestampSection(undefined)).toBe(false);
    expect(timestampSectionStartSeconds(undefined)).toBeNull();
  });
});

describe('approximateScrollRatio', () => {
  it('is zero at the start of a document', () => {
    expect(approximateScrollRatio(0)).toBe(0);
    expect(approximateScrollRatio(-3)).toBe(0);
  });

  it('grows with the chunk ordinal', () => {
    expect(approximateScrollRatio(4)).toBeCloseTo(0.5);
    expect(approximateScrollRatio(16)).toBeGreaterThan(approximateScrollRatio(4));
  });

  it('never claims the very end of the document', () => {
    expect(approximateScrollRatio(100_000)).toBeLessThanOrEqual(0.95);
  });
});

describe('rememberLocation / recallLocation', () => {
  beforeEach(() => {
    forgetAllLocations();
  });

  it('round-trips a resolved label', () => {
    rememberLocation('chunk-1', 'p. 12');
    expect(recallLocation('chunk-1')).toBe('p. 12');
  });

  it('returns undefined for an unknown chunk', () => {
    expect(recallLocation('chunk-unknown')).toBeUndefined();
  });

  it('ignores empty inputs rather than storing junk', () => {
    rememberLocation('', 'p. 1');
    rememberLocation('chunk-2', '');
    expect(recallLocation('')).toBeUndefined();
    expect(recallLocation('chunk-2')).toBeUndefined();
  });
});

it('labels stored physical PDF pages distinctly from printed page labels', () => {
  expect(formatSourceLocation({ section: '706.07', chunkId: 'c', pageNumber: 42 })).toBe('PDF p. 42');
});

// A query-centred preview must never replace the evidence passage sent to the model.
it('locates the full cited passage instead of the search snippet', () => {
  const source = {
    chunkId: 'passage-2', content: 'A complete passage explaining the patent filing requirements.',
    excerpt: '…patent…', highlights: ['patent'],
    chunkExcerpts: [{ chunkId: 'passage-2', excerpt: '…patent…' }],
  } as Parameters<typeof locatorFromSource>[0];
  expect(locatorFromSource(source, 'passage-2').text).toBe(source.content);
});
