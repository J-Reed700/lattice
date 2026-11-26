import type { TextSelector } from './types';

const encoder = new TextEncoder();

/** Bytes of the UTF-8 encoding of text.slice(0, index). An index inside a surrogate pair counts from the pair's start. */
export function utf16ToByte(text: string, index: number): number {
  let clamped = Math.max(0, Math.min(index, text.length));
  if (clamped > 0 && clamped < text.length) {
    const code = text.charCodeAt(clamped);
    const prev = text.charCodeAt(clamped - 1);
    if (code >= 0xdc00 && code <= 0xdfff && prev >= 0xd800 && prev <= 0xdbff) {
      clamped -= 1;
    }
  }
  return encoder.encode(text.slice(0, clamped)).length;
}

/** The UTF-16 index at that byte; null when the byte is past the end or inside a character. */
export function byteToUtf16(text: string, byte: number): number | null {
  if (byte < 0 || !Number.isInteger(byte)) return null;
  if (byte === 0) return 0;

  let bytes = 0;
  for (let i = 0; i < text.length;) {
    const cp = text.codePointAt(i);
    if (cp === undefined) break;
    const utf16Len = cp > 0xffff ? 2 : 1;
    const utf8Len = cp <= 0x7f ? 1 : cp <= 0x7ff ? 2 : cp <= 0xffff ? 3 : 4;
    bytes += utf8Len;
    i += utf16Len;
    if (bytes === byte) return i;
    if (bytes > byte) return null;
  }
  return null;
}

/** The UTF-16 range a selector names in value; null unless value.slice(start, end) === selector.exact. */
export function rangeOf(value: string, selector: TextSelector): [number, number] | null {
  const start = byteToUtf16(value, selector.startByte);
  const end = byteToUtf16(value, selector.endByte);
  if (start === null || end === null || start > end) return null;
  if (value.slice(start, end) !== selector.exact) return null;
  return [start, end];
}
