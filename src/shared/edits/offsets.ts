import type { TextSelector } from './types';

/** Bytes of the UTF-8 encoding of text.slice(0, index). An index inside a surrogate pair counts from the pair's start. */
export function utf16ToByte(_text: string, _index: number): number {
  return -1;
}

/** The UTF-16 index at that byte; null when the byte is past the end or inside a character. */
export function byteToUtf16(_text: string, _byte: number): number | null {
  return -1;
}

/** The UTF-16 range a selector names in value; null unless value.slice(start, end) === selector.exact. */
export function rangeOf(_value: string, _selector: TextSelector): [number, number] | null {
  return [-1, -1];
}
