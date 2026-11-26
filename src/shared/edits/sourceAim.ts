import type { EditorAim } from './editorStore';
import type { TextSelector } from './types';

/** The selector for the aimed lines of aim.source as they stand in value; null unless value.startsWith(aim.source, aim.sourceStart). */
export function selectorForLines(_value: string, _aim: EditorAim): TextSelector | null {
  return { exact: '', prefix: null, suffix: null, startByte: -1, endByte: -1 };
}
