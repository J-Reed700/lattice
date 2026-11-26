import type { EditOp, FieldSpec, JsonValue } from './types';

export type DraftMember = { origin: number | null; value: Record<string, JsonValue> };
export type DraftState = { values: Record<string, JsonValue>; lists: Record<string, DraftMember[]> };

function parsePointer(pointer: string): string[] {
  if (!pointer || pointer === '/') return [];
  return pointer
    .replace(/^\//, '')
    .split('/')
    .map((segment) => segment.replace(/~1/g, '/').replace(/~0/g, '~'));
}

export function joinPointer(...parts: string[]): string {
  const segments: string[] = [];
  for (const part of parts) {
    const trimmed = part.replace(/^\/+|\/+$/g, '');
    if (trimmed !== '') segments.push(trimmed);
  }
  return `/${segments.join('/')}`;
}

export function readPointer(root: JsonValue, pointer: string): JsonValue | undefined {
  const segments = parsePointer(pointer);
  let cursor: JsonValue | undefined = root;
  for (const segment of segments) {
    if (cursor === null || typeof cursor !== 'object') return undefined;
    if (Array.isArray(cursor)) {
      const idx = Number(segment);
      if (!Number.isInteger(idx) || idx < 0 || idx >= cursor.length) return undefined;
      cursor = cursor[idx];
    } else {
      cursor = cursor[segment];
    }
  }
  return cursor;
}

export function writePointer(root: Record<string, JsonValue>, pointer: string, value: JsonValue): void {
  const segments = parsePointer(pointer);
  if (segments.length === 0) return;
  let cursor: Record<string, JsonValue> = root;
  for (let i = 0; i < segments.length - 1; i += 1) {
    const segment = segments[i];
    const next = cursor[segment];
    if (next === null || typeof next !== 'object' || Array.isArray(next)) {
      const created: Record<string, JsonValue> = {};
      cursor[segment] = created;
      cursor = created;
    } else {
      cursor = next;
    }
  }
  cursor[segments[segments.length - 1]] = value;
}

export function structuredCloneJson<T extends JsonValue>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export function defaultValueForField(field: FieldSpec): JsonValue {
  switch (field.type) {
    case 'text':
      return '';
    case 'flag':
      return false;
    case 'choice':
      return 0;
    case 'number':
      return field.min;
    case 'list':
      return [];
    default: {
      const exhaustive: never = field;
      return exhaustive;
    }
  }
}

export function defaultMemberValue(item: FieldSpec[]): Record<string, JsonValue> {
  const member: Record<string, JsonValue> = {};
  for (const leaf of item) {
    writePointer(member, leaf.path, defaultValueForField(leaf));
  }
  return member;
}

export function valuesEqual(a: JsonValue | undefined, b: JsonValue | undefined): boolean {
  if (typeof a === 'string' && typeof b === 'string') {
    return a.replace(/\r\n/g, '\n') === b.replace(/\r\n/g, '\n');
  }
  if (a === b) return true;
  if (a === null || b === null || typeof a !== 'object' || typeof b !== 'object') return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v, idx) => valuesEqual(v, b[idx]));
  }
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return false;
  return keysA.every((key) => Object.prototype.hasOwnProperty.call(b, key) && valuesEqual(a[key], b[key]));
}

function cloneMemberFromItem(item: FieldSpec[], raw: JsonValue): Record<string, JsonValue> {
  const member: Record<string, JsonValue> = {};
  for (const leaf of item) {
    const value = readPointer(raw, leaf.path);
    writePointer(member, leaf.path, value !== undefined ? structuredCloneJson(value) : defaultValueForField(leaf));
  }
  return member;
}

export function draftOf(form: FieldSpec[], document: JsonValue): DraftState {
  const values: Record<string, JsonValue> = {};
  const lists: Record<string, DraftMember[]> = {};

  for (const field of form) {
    const raw = readPointer(document, field.path);
    if (field.type === 'list') {
      const items = Array.isArray(raw) ? raw : [];
      lists[field.path] = items.map((item, origin) => ({
        origin,
        value: cloneMemberFromItem(field.item, item),
      }));
    } else {
      values[field.path] = raw !== undefined ? structuredCloneJson(raw) : defaultValueForField(field);
    }
  }

  return { values, lists };
}

export function documentOf(form: FieldSpec[], draft: DraftState): JsonValue {
  const doc: Record<string, JsonValue> = {};

  for (const field of form) {
    if (field.type === 'list') {
      const members = draft.lists[field.path] ?? [];
      writePointer(
        doc,
        field.path,
        members.map((member) => structuredCloneJson(member.value)),
      );
    } else {
      const value = draft.values[field.path];
      writePointer(
        doc,
        field.path,
        value !== undefined ? structuredCloneJson(value) : defaultValueForField(field),
      );
    }
  }

  return doc;
}

export function diffDocument(form: FieldSpec[], opened: JsonValue, draft: DraftState): EditOp[] {
  const ops: EditOp[] = [];

  for (const field of form) {
    if (field.type !== 'list') {
      const openedValue = readPointer(opened, field.path);
      const draftValue = draft.values[field.path];
      if (draftValue !== undefined && !valuesEqual(openedValue, draftValue)) {
        ops.push({ op: 'replace', path: field.path, value: structuredCloneJson(draftValue) });
      }
      continue;
    }

    const rawOpened = readPointer(opened, field.path);
    const openedList = Array.isArray(rawOpened) ? rawOpened : [];
    const draftMembers = draft.lists[field.path] ?? [];

    const keptOrigins = new Set<number>();
    for (const member of draftMembers) {
      if (member.origin !== null) keptOrigins.add(member.origin);
    }

    for (let idx = openedList.length - 1; idx >= 0; idx -= 1) {
      if (!keptOrigins.has(idx)) {
        ops.push({ op: 'remove', path: joinPointer(field.path, String(idx)) });
      }
    }

    let keptIndex = 0;
    for (const member of draftMembers) {
      if (member.origin === null) continue;
      const openedItem = openedList[member.origin];
      for (const leaf of field.item) {
        if (leaf.type === 'list') continue;
        const openedLeaf = openedItem !== undefined ? readPointer(openedItem, leaf.path) : undefined;
        const draftLeaf = readPointer(member.value, leaf.path);
        if (draftLeaf !== undefined && !valuesEqual(openedLeaf, draftLeaf)) {
          ops.push({
            op: 'replace',
            path: joinPointer(field.path, String(keptIndex), leaf.path),
            value: structuredCloneJson(draftLeaf),
          });
        }
      }
      keptIndex += 1;
    }

    for (const member of draftMembers) {
      if (member.origin !== null) continue;
      ops.push({
        op: 'add',
        path: joinPointer(field.path, '-'),
        value: structuredCloneJson(member.value),
      });
    }
  }

  return ops;
}

/** The draft against another document: member i takes origin i while i is below that document's list length, null above. */
export function rebased(form: FieldSpec[], current: JsonValue, draft: DraftState): DraftState {
  const values: Record<string, JsonValue> = { ...draft.values };
  const lists: Record<string, DraftMember[]> = {};

  for (const field of form) {
    if (field.type !== 'list') {
      if (!(field.path in values)) {
        const curValue = readPointer(current, field.path);
        values[field.path] = curValue !== undefined ? structuredCloneJson(curValue) : defaultValueForField(field);
      }
      continue;
    }

    const rawCurrent = readPointer(current, field.path);
    const currentLength = Array.isArray(rawCurrent) ? rawCurrent.length : 0;
    const members = draft.lists[field.path] ?? [];
    lists[field.path] = members.map((member, index) => ({
      origin: index < currentLength ? index : null,
      value: structuredCloneJson(member.value),
    }));
  }

  return { values, lists };
}
