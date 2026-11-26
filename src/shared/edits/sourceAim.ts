import { utf16ToByte } from './offsets';

import type { EditorAim } from './editorStore';
import type { TextSelector } from './types';

/** The selector for the aimed lines of aim.source as they stand in value; null unless value.startsWith(aim.source, aim.sourceStart). */
export function selectorForLines(value: string, aim: EditorAim): TextSelector | null {
  if (aim.sourceStart < 0 || !value.startsWith(aim.source, aim.sourceStart)) return null;

  const lines = aim.source.split('\n');
  const [first, rawEnd] = aim.lines;
  let end = rawEnd;
  if (first < 0 || end <= first || end > lines.length) return null;

  while (end - first > 1 && lines[end - 1].replace(/\r$/, '') === '') {
    end -= 1;
  }

  let relStart = 0;
  for (let i = 0; i < first; i += 1) {
    relStart += lines[i].length + 1;
  }

  let lastLineStart = relStart;
  for (let i = first; i < end - 1; i += 1) {
    lastLineStart += lines[i].length + 1;
  }

  const lastLine = lines[end - 1];
  const lastLineLen = lastLine.endsWith('\r') ? lastLine.length - 1 : lastLine.length;
  const relEnd = lastLineStart + lastLineLen;

  const absStart = aim.sourceStart + relStart;
  const absEnd = aim.sourceStart + relEnd;

  return {
    exact: value.slice(absStart, absEnd),
    prefix: null,
    suffix: null,
    startByte: utf16ToByte(value, absStart),
    endByte: utf16ToByte(value, absEnd),
  };
}
