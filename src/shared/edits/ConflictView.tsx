import { Button } from '@/components/ui/button';

import { joinPointer, readPointer, valuesEqual, type DraftState } from './diffDocument';
import { formatFieldValue } from './formatValue';

import type { EditableContent, JsonValue } from './types';

export interface ConflictViewProps {
  current: EditableContent;
  draft: DraftState;
  saving: boolean;
  error: string | null;
  onSaveOver: () => void;
  onDiscard: () => void;
}

type DiffRow = {
  key: string;
  label: string;
  nowText: string;
  draftText: string;
};

function collectDiffRows(current: EditableContent, draft: DraftState): DiffRow[] {
  const rows: DiffRow[] = [];

  for (const field of current.form) {
    if (field.type !== 'list') {
      const nowVal = readPointer(current.document, field.path);
      const draftVal = draft.values[field.path];
      if (!valuesEqual(nowVal, draftVal)) {
        rows.push({
          key: field.path,
          label: field.label,
          nowText: formatFieldValue(nowVal, current.form, field.path),
          draftText: formatFieldValue(draftVal, current.form, field.path),
        });
      }
      continue;
    }

    const rawNow = readPointer(current.document, field.path);
    const nowList = Array.isArray(rawNow) ? rawNow : [];
    const draftList = draft.lists[field.path] ?? [];
    const count = Math.max(nowList.length, draftList.length);

    for (let index = 0; index < count; index += 1) {
      const nowItem: JsonValue | undefined = index < nowList.length ? nowList[index] : undefined;
      const draftItem = index < draftList.length ? draftList[index].value : undefined;
      for (const leaf of field.item) {
        const fullPath = joinPointer(field.path, String(index), leaf.path);
        const nowLeaf = nowItem !== undefined ? readPointer(nowItem, leaf.path) : undefined;
        const draftLeaf = draftItem !== undefined ? readPointer(draftItem, leaf.path) : undefined;
        if (!valuesEqual(nowLeaf, draftLeaf)) {
          rows.push({
            key: fullPath,
            label: `${field.itemLabel} ${index + 1}: ${leaf.label}`,
            nowText: formatFieldValue(nowLeaf, current.form, fullPath),
            draftText: formatFieldValue(draftLeaf, current.form, fullPath),
          });
        }
      }
    }
  }

  return rows;
}

export function ConflictView({
  current,
  draft,
  saving,
  error,
  onSaveOver,
  onDiscard,
}: ConflictViewProps) {
  const rows = collectDiffRows(current, draft);

  return (
    <div className="space-y-4">
      <div
        role="alert"
        className="rounded-md border border-[hsl(var(--border-default))] bg-[hsl(var(--surface-sunken))] p-3 text-ui font-medium text-[hsl(var(--danger-fg))]"
      >
        This changed since you opened it.
      </div>

      {rows.length > 0 && (
        <div className="space-y-3">
          {rows.map((row) => (
            <div key={row.key} className="space-y-1.5 rounded-md border border-[hsl(var(--border-subtle))] p-3">
              <div className="text-xs font-medium text-[hsl(var(--text-secondary))]">{row.label}</div>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div className="rounded bg-[hsl(var(--surface-sunken))] p-2">
                  <div className="text-xs font-medium text-[hsl(var(--text-tertiary))]">Now</div>
                  <div className="mt-1 whitespace-pre-wrap text-ui text-[hsl(var(--text-primary))]">{row.nowText}</div>
                </div>
                <div className="rounded bg-[hsl(var(--surface-sunken))] p-2">
                  <div className="text-xs font-medium text-[hsl(var(--text-tertiary))]">Your draft</div>
                  <div className="mt-1 whitespace-pre-wrap text-ui text-[hsl(var(--text-primary))]">{row.draftText}</div>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {error ? (
        <p role="alert" className="text-xs text-[hsl(var(--danger-fg))]">
          {error}
        </p>
      ) : null}

      <div className="flex flex-wrap justify-end gap-2">
        <Button type="button" variant="outline" disabled={saving} onClick={onDiscard}>
          Discard my draft
        </Button>
        <Button type="button" disabled={saving} onClick={onSaveOver}>
          Save my draft over this
        </Button>
      </div>
    </div>
  );
}
