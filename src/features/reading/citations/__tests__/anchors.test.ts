import { describe, expect, it } from 'vitest';

import type { AnchorDto } from '@/lib/bindings';

import { placeAnchors, utf16Span } from '../anchors';

const bytesBefore = (text: string, index: number): number => new TextEncoder().encode(text.slice(0, index)).length;

/** An anchor on `exact`'s `copy`-th appearance in `text`, its span in bytes counted from `base`. */
function anchorOn(text: string, exact: string, copy = 0, base = 0): AnchorDto {
  let at = -1;
  for (let seen = 0; seen <= copy; seen += 1) at = text.indexOf(exact, at + 1);
  return {
    exact,
    span: { start: base + bytesBefore(text, at), end: base + bytesBefore(text, at + exact.length) },
  };
}

describe('utf16Span', () => {
  it('counts a two-byte letter as one UTF-16 unit', () => {
    expect(utf16Span('aéb', 1, 3)).toEqual({ start: 1, end: 2 });
    expect(utf16Span('aéb', 3, 4)).toEqual({ start: 2, end: 3 });
  });

  it('counts an emoji as four bytes and two UTF-16 units', () => {
    expect(utf16Span('😀x', 0, 4)).toEqual({ start: 0, end: 2 });
    expect(utf16Span('😀x', 4, 5)).toEqual({ start: 2, end: 3 });
  });

  it('counts CJK characters as three bytes each', () => {
    expect(utf16Span('日本語', 3, 6)).toEqual({ start: 1, end: 2 });
  });

  it('gives none for a byte inside a character', () => {
    expect(utf16Span('aéb', 2, 3)).toBeNull();
    expect(utf16Span('😀x', 1, 4)).toBeNull();
    expect(utf16Span('日本語', 3, 5)).toBeNull();
  });

  it('gives none for a span past the end or a reversed one', () => {
    expect(utf16Span('abc', 0, 99)).toBeNull();
    expect(utf16Span('abc', 2, 1)).toBeNull();
  });

  it('reads the whole text as one span', () => {
    expect(utf16Span('aé😀', 0, 7)).toEqual({ start: 0, end: 4 });
  });
});

describe('placeAnchors', () => {
  const text = 'Café ☕ opens early. The tea is hot. Tea is hot too.';

  it('places an anchor where its byte span says, after multi-byte characters', () => {
    const anchor = anchorOn(text, 'The tea is hot.');
    const { ranges, missing } = placeAnchors(text, [anchor]);

    expect(missing).toEqual([]);
    expect(ranges).toHaveLength(1);
    expect(text.slice(ranges[0]!.start, ranges[0]!.end)).toBe('The tea is hot.');
  });

  it('reads the span from the byte where the text begins', () => {
    const passage = 'Café ☕ is open. Tea is hot. Tea is hot.';
    const anchor = anchorOn(passage, 'Tea is hot.', 1, 4000);
    const { ranges } = placeAnchors(passage, [anchor], 4000);

    expect(ranges).toEqual([{ start: passage.lastIndexOf('Tea is hot.'), end: passage.length }]);
  });

  it('falls back to the copy of the words nearest the expected place when the span holds other words', () => {
    const twice = 'Tea is hot. Something else entirely. Tea is hot. Last words.';
    const second = anchorOn(twice, 'Tea is hot.', 1);
    const off = { ...second, span: { start: second.span!.start + 2, end: second.span!.end + 2 } };
    const { ranges, missing } = placeAnchors(twice, [off]);

    expect(missing).toEqual([]);
    expect(ranges).toEqual([{ start: twice.lastIndexOf('Tea is hot.'), end: twice.lastIndexOf('Tea is hot.') + 11 }]);
  });

  it('takes the first copy of the words when the anchor has no span', () => {
    const twice = 'Tea is hot. Something else entirely. Tea is hot.';
    const { ranges } = placeAnchors(twice, [{ exact: 'Tea is hot.' }]);

    expect(ranges).toEqual([{ start: 0, end: 11 }]);
  });

  it('reports an anchor whose words are not in the text as missing', () => {
    const gone: AnchorDto = { exact: 'Never written here.', span: { start: 0, end: 19 } };
    const { ranges, missing } = placeAnchors(text, [gone, anchorOn(text, 'Café ☕ opens early.')]);

    expect(missing).toEqual([gone]);
    expect(ranges).toEqual([{ start: 0, end: 'Café ☕ opens early.'.length }]);
  });

  it('returns ranges in text order, merged where they touch or overlap', () => {
    const first = anchorOn(text, 'Café ☕ opens early.');
    const second = anchorOn(text, ' The tea is hot.');
    const last = anchorOn(text, 'Tea is hot too.');
    const { ranges } = placeAnchors(text, [last, second, first]);

    expect(ranges).toEqual([
      { start: 0, end: text.indexOf(' Tea is hot too.') },
      { start: text.indexOf('Tea is hot too.'), end: text.length },
    ]);
  });
});
