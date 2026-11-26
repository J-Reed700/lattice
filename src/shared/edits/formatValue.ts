import { joinPointer } from './diffDocument';

import type { FieldSpec, JsonValue } from './types';

function choiceFieldForPath(form: FieldSpec[] | undefined, path: string): Extract<FieldSpec, { type: 'choice' }> | undefined {
  if (!form) return undefined;
  for (const field of form) {
    if (field.type === 'choice' && field.path === path) return field;
    if (field.type === 'list' && path.startsWith(`${field.path}/`)) {
      const rest = path.slice(field.path.length + 1);
      const slash = rest.indexOf('/');
      if (slash === -1) continue;
      const leafPath = rest.slice(slash);
      const leaf = field.item.find((item) => item.type === 'choice' && item.path === leafPath);
      if (leaf?.type === 'choice') return leaf;
    }
  }
  return undefined;
}

export function formatFieldValue(
  value: JsonValue | undefined,
  form?: FieldSpec[],
  path?: string,
): string {
  if (value === null || value === undefined) return 'None';
  if (typeof value === 'boolean') return value ? 'Yes' : 'No';
  if (typeof value === 'number') {
    const choice = path ? choiceFieldForPath(form, path) : undefined;
    if (choice && value >= 0 && value < choice.options.length) {
      return choice.options[value];
    }
    return String(value);
  }
  if (typeof value === 'string') return value;
  return JSON.stringify(value);
}

export function fieldLabelForPath(form: FieldSpec[] | undefined, path: string): string {
  if (!form) return path;
  for (const field of form) {
    if (field.path === path) return field.label;
    if (field.type === 'list' && path.startsWith(`${field.path}/`)) {
      const rest = path.slice(field.path.length + 1);
      const slash = rest.indexOf('/');
      const indexText = slash === -1 ? rest : rest.slice(0, slash);
      const idx = Number(indexText);
      const oneBased = Number.isInteger(idx) ? idx + 1 : indexText;
      if (slash === -1) return `${field.itemLabel} ${oneBased}`;
      const leafPath = joinPointer(rest.slice(slash));
      const leaf = field.item.find((item) => item.path === leafPath);
      return `${field.itemLabel} ${oneBased}: ${leaf?.label ?? leafPath}`;
    }
  }
  return path;
}
