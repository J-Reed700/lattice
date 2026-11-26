import { describe, expect, it } from 'vitest';

import { byteToUtf16, rangeOf, utf16ToByte } from './offsets';

describe('offsets', () => {
  const sample = 'aé—𝄞b';

  it('converts between UTF-16 indices and UTF-8 bytes across multi-byte characters', () => {
    expect(utf16ToByte(sample, 0)).toBe(0);
    expect(utf16ToByte(sample, 1)).toBe(1); // after 'a' (1 byte)
    expect(utf16ToByte(sample, 2)).toBe(3); // after 'é' (2 bytes)
    expect(utf16ToByte(sample, 3)).toBe(6); // after '—' (3 bytes)
    expect(utf16ToByte(sample, 4)).toBe(6); // inside surrogate pair '𝄞' counts from start
    expect(utf16ToByte(sample, 5)).toBe(10); // after '𝄞' (4 bytes)
    expect(utf16ToByte(sample, 6)).toBe(11); // after 'b' (1 byte)

    expect(byteToUtf16(sample, 0)).toBe(0);
    expect(byteToUtf16(sample, 1)).toBe(1);
    expect(byteToUtf16(sample, 3)).toBe(2);
    expect(byteToUtf16(sample, 6)).toBe(3);
    expect(byteToUtf16(sample, 10)).toBe(5);
    expect(byteToUtf16(sample, 11)).toBe(6);
  });

  it('returns null when a byte falls inside a character or past the end', () => {
    expect(byteToUtf16(sample, 2)).toBeNull(); // inside 2-byte 'é'
    expect(byteToUtf16(sample, 4)).toBeNull(); // inside 3-byte '—'
    expect(byteToUtf16(sample, 5)).toBeNull();
    expect(byteToUtf16(sample, 7)).toBeNull(); // inside 4-byte '𝄞'
    expect(byteToUtf16(sample, 8)).toBeNull();
    expect(byteToUtf16(sample, 9)).toBeNull();
    expect(byteToUtf16(sample, 12)).toBeNull(); // past end
    expect(byteToUtf16(sample, -1)).toBeNull();
  });

  it('resolves rangeOf only when the byte span holds the exact text', () => {
    const startByte = utf16ToByte(sample, 2);
    const endByte = utf16ToByte(sample, 5);
    expect(rangeOf(sample, { exact: '—𝄞', prefix: null, suffix: null, startByte, endByte })).toEqual([2, 5]);
    expect(rangeOf(sample, { exact: 'wrong', prefix: null, suffix: null, startByte, endByte })).toBeNull();
    expect(rangeOf('aé—XYb', { exact: '—𝄞', prefix: null, suffix: null, startByte, endByte })).toBeNull();
  });
});
