import type { EditOp, FieldSpec, JsonValue } from './types';

export type DraftMember = { origin: number | null; value: Record<string, JsonValue> };
export type DraftState = { values: Record<string, JsonValue>; lists: Record<string, DraftMember[]> };

export function draftOf(_form: FieldSpec[], _document: JsonValue): DraftState {
  return { values: {}, lists: {} };
}

export function documentOf(_form: FieldSpec[], _draft: DraftState): JsonValue {
  return {};
}

export function diffDocument(_form: FieldSpec[], _opened: JsonValue, _draft: DraftState): EditOp[] {
  return [{ op: 'remove', path: '/__stub__' }];
}

/** The draft against another document: member i takes origin i while i is below that document's list length, null above. */
export function rebased(_form: FieldSpec[], _current: JsonValue, _draft: DraftState): DraftState {
  return { values: {}, lists: {} };
}
