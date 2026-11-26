import type { AnchorDto } from '@/lib/bindings';

/** A range of UTF-16 code units of a string. */
export interface TextSpan {
  start: number;
  end: number;
}

/**
 * The UTF-16 range of `text` that holds bytes `[startByte, endByte)` of its UTF-8 form.
 * None when either byte falls inside a character or past the end.
 */
export function utf16Span(_text: string, _startByte: number, _endByte: number): TextSpan | null {
  return null;
}

/**
 * Where each anchor lies in `text`, which begins at byte `base` of the saved version.
 *
 * An anchor sits where its byte span says when the words there are its own; else at the copy of its
 * words nearest that place; else it is missing. Ranges come back in text order, merged where they touch.
 */
export function placeAnchors(
  _text: string,
  _anchors: readonly AnchorDto[],
  _base = 0,
): { ranges: TextSpan[]; missing: AnchorDto[] } {
  return { ranges: [], missing: [] };
}
