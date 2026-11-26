import type { AnchorDto } from '@/lib/bindings';

/** A range of UTF-16 code units of a string. */
export interface TextSpan {
  start: number;
  end: number;
}

/** The UTF-8 length of one code point. */
function utf8Length(codePoint: number): number {
  if (codePoint < 0x80) return 1;
  if (codePoint < 0x800) return 2;
  if (codePoint < 0x10000) return 3;
  return 4;
}

/**
 * The UTF-16 range of `text` that holds bytes `[startByte, endByte)` of its UTF-8 form.
 * None when either byte falls inside a character or past the end.
 */
export function utf16Span(text: string, startByte: number, endByte: number): TextSpan | null {
  if (!Number.isInteger(startByte) || !Number.isInteger(endByte) || startByte < 0 || endByte < startByte) {
    return null;
  }
  let start: number | null = startByte === 0 ? 0 : null;
  let end: number | null = endByte === 0 ? 0 : null;
  let byte = 0;
  let unit = 0;
  for (const char of text) {
    if (byte >= endByte) break;
    byte += utf8Length(char.codePointAt(0) ?? 0);
    unit += char.length;
    if (byte === startByte) start = unit;
    if (byte === endByte) end = unit;
  }
  return start !== null && end !== null ? { start, end } : null;
}

/** The copy of `exact` in `text` whose start is nearest `expected`, or the first when there is no expectation. */
function nearestCopy(text: string, exact: string, expected: number | null): TextSpan | null {
  let best: TextSpan | null = null;
  let from = text.indexOf(exact);
  while (from >= 0) {
    if (expected === null) return { start: from, end: from + exact.length };
    if (best === null || Math.abs(from - expected) < Math.abs(best.start - expected)) {
      best = { start: from, end: from + exact.length };
    }
    from = text.indexOf(exact, from + 1);
  }
  return best;
}

/**
 * Where each anchor lies in `text`, which begins at byte `base` of the saved version.
 *
 * An anchor sits where its byte span says when the words there are its own; else at the copy of its
 * words nearest that place; else it is missing. Ranges come back in text order, merged where they touch.
 */
export function placeAnchors(
  text: string,
  anchors: readonly AnchorDto[],
  base = 0,
): { ranges: TextSpan[]; missing: AnchorDto[] } {
  const placed: TextSpan[] = [];
  const missing: AnchorDto[] = [];
  for (const anchor of anchors) {
    let found: TextSpan | null = null;
    let expected: number | null = null;
    if (anchor.span) {
      const span = utf16Span(text, anchor.span.start - base, anchor.span.end - base);
      if (span) {
        expected = span.start;
        if (text.slice(span.start, span.end) === anchor.exact) found = span;
      }
    }
    if (!found && anchor.exact) found = nearestCopy(text, anchor.exact, expected);
    if (found) placed.push(found);
    else missing.push(anchor);
  }
  placed.sort((a, b) => a.start - b.start || a.end - b.end);
  const ranges: TextSpan[] = [];
  for (const span of placed) {
    const last = ranges[ranges.length - 1];
    if (last && span.start <= last.end) last.end = Math.max(last.end, span.end);
    else ranges.push({ ...span });
  }
  return { ranges, missing };
}
