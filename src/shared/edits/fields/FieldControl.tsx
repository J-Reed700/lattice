import { useId, type ReactElement } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { Button } from '@/components/ui/button';

import {
  defaultMemberValue,
  joinPointer,
  readPointer,
  structuredCloneJson,
  writePointer,
  type DraftMember,
  type DraftState,
} from '../diffDocument';

import type { FieldNotice, FieldProblem, FieldSpec, JsonValue } from '../types';

export function isValidNumberValue(value: JsonValue | undefined, min: number): boolean {
  return typeof value === 'number' && Number.isInteger(value) && value >= min;
}

export function hasInvalidNumberField(form: FieldSpec[], draft: DraftState): boolean {
  for (const field of form) {
    if (field.type === 'number') {
      if (!isValidNumberValue(draft.values[field.path], field.min)) return true;
    } else if (field.type === 'list') {
      const members = draft.lists[field.path] ?? [];
      for (const member of members) {
        for (const leaf of field.item) {
          if (leaf.type === 'number' && !isValidNumberValue(readPointer(member.value, leaf.path), leaf.min)) {
            return true;
          }
        }
      }
    }
  }
  return false;
}

export interface FieldControlProps {
  field: FieldSpec;
  fullPath: string;
  value: JsonValue;
  draftMembers?: DraftMember[];
  onChangeValue: (value: JsonValue) => void;
  onChangeList?: (members: DraftMember[]) => void;
  onExclusiveToggle?: () => void;
  exclusiveGroupName?: string;
  readOnly: boolean;
  editedPaths: readonly string[];
  problems: readonly FieldProblem[];
  notices: readonly FieldNotice[];
  registerControl?: (path: string, element: HTMLElement | null) => void;
}

const inputClasses =
  'w-full rounded-md border border-[hsl(var(--border-default))] bg-[hsl(var(--surface))] px-3 py-1.5 text-ui text-[hsl(var(--text-primary))] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))]';

export function FieldControl({
  field,
  fullPath,
  value,
  draftMembers = [],
  onChangeValue,
  onChangeList,
  onExclusiveToggle,
  exclusiveGroupName,
  readOnly,
  editedPaths,
  problems,
  notices,
  registerControl,
}: FieldControlProps): ReactElement {
  const controlId = useId();
  const labelId = useId();
  const problemId = useId();
  const numberErrorId = useId();

  const isEdited = editedPaths.includes(fullPath);
  const problem = problems.find((item) => item.path === fullPath);
  const fieldNotices = notices.filter((item) => item.path === fullPath);

  const editedBadge = isEdited ? (
    <span className="ml-2 text-xs font-normal text-[hsl(var(--text-tertiary))]">Edited by you</span>
  ) : null;

  const feedbackBlock = (invalidMessage?: string) => (
    <>
      {invalidMessage ? (
        <p id={numberErrorId} role="alert" className="text-xs text-[hsl(var(--danger-fg))]">
          {invalidMessage}
        </p>
      ) : null}
      {problem ? (
        <p id={problemId} role="alert" className="text-xs text-[hsl(var(--danger-fg))]">
          {problem.message}
        </p>
      ) : null}
      {fieldNotices.map((notice) => (
        <p key={`${notice.code}:${notice.line ?? 'none'}`} role="status" className="text-xs text-[hsl(var(--text-secondary))]">
          {notice.message}
        </p>
      ))}
    </>
  );

  switch (field.type) {
    case 'text': {
      const normalized = typeof value === 'string' ? value.replace(/\r\n/g, '\n') : '';
      const describedBy = problem ? problemId : undefined;
      return (
        <div className="space-y-1.5">
          <label htmlFor={controlId} className="block text-ui font-medium text-[hsl(var(--text-primary))]">
            {field.label}
            {editedBadge}
          </label>
          {field.format === 'line' && (
            <input
              ref={(el) => registerControl?.(fullPath, el)}
              id={controlId}
              type="text"
              className={inputClasses}
              value={normalized}
              readOnly={readOnly}
              aria-invalid={Boolean(problem) || undefined}
              aria-describedby={describedBy}
              onChange={(event) => onChangeValue(event.target.value)}
            />
          )}
          {field.format === 'plain' && (
            <textarea
              ref={(el) => registerControl?.(fullPath, el)}
              id={controlId}
              rows={4}
              className={inputClasses}
              value={normalized}
              readOnly={readOnly}
              aria-invalid={Boolean(problem) || undefined}
              aria-describedby={describedBy}
              onChange={(event) => onChangeValue(event.target.value)}
            />
          )}
          {field.format === 'markdown' && (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <textarea
                ref={(el) => registerControl?.(fullPath, el)}
                id={controlId}
                rows={8}
                className={`${inputClasses} font-mono text-xs`}
                value={normalized}
                readOnly={readOnly}
                aria-invalid={Boolean(problem) || undefined}
                aria-describedby={describedBy}
                onChange={(event) => onChangeValue(event.target.value)}
              />
              <section
                aria-label="Preview"
                className="min-h-[10rem] overflow-auto rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-sunken))] p-3"
              >
                <TiptapViewer content={normalized} />
              </section>
            </div>
          )}
          {feedbackBlock()}
        </div>
      );
    }

    case 'flag': {
      const checked = Boolean(value);
      const describedBy = problem ? problemId : undefined;
      if (field.exclusive && exclusiveGroupName) {
        return (
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <input
                ref={(el) => registerControl?.(fullPath, el)}
                id={controlId}
                type="radio"
                name={exclusiveGroupName}
                checked={checked}
                disabled={readOnly}
                aria-describedby={describedBy}
                onChange={() => onExclusiveToggle?.()}
              />
              <label htmlFor={controlId} className="text-ui text-[hsl(var(--text-primary))]">
                {field.label}
                {editedBadge}
              </label>
            </div>
            {feedbackBlock()}
          </div>
        );
      }

      return (
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <input
              ref={(el) => registerControl?.(fullPath, el)}
              id={controlId}
              type="checkbox"
              checked={checked}
              disabled={readOnly}
              aria-describedby={describedBy}
              onChange={(event) => onChangeValue(event.target.checked)}
            />
            <label htmlFor={controlId} className="text-ui text-[hsl(var(--text-primary))]">
              {field.label}
              {editedBadge}
            </label>
          </div>
          {feedbackBlock()}
        </div>
      );
    }

    case 'choice': {
      const selected = typeof value === 'number' ? value : 0;
      const describedBy = problem ? problemId : undefined;
      return (
        <div className="space-y-1.5">
          <div role="radiogroup" aria-labelledby={labelId} aria-describedby={describedBy} className="space-y-1.5">
            <span id={labelId} className="block text-ui font-medium text-[hsl(var(--text-primary))]">
              {field.label}
              {editedBadge}
            </span>
            <div className="flex flex-wrap gap-3">
              {field.options.map((option, optionIndex) => (
                <label key={`${optionIndex}:${option}`} className="inline-flex items-center gap-1.5 text-ui text-[hsl(var(--text-primary))]">
                  <input
                    ref={optionIndex === 0 ? (el) => registerControl?.(fullPath, el) : undefined}
                    type="radio"
                    name={`${controlId}:choice`}
                    checked={selected === optionIndex}
                    disabled={readOnly}
                    onChange={() => onChangeValue(optionIndex)}
                  />
                  <span>{option}</span>
                </label>
              ))}
            </div>
          </div>
          {feedbackBlock()}
        </div>
      );
    }

    case 'number': {
      const valid = isValidNumberValue(value, field.min);
      const invalidMessage = valid
        ? undefined
        : `${field.label} must be a whole number, ${field.min} or more.`;
      const describedBy = [invalidMessage ? numberErrorId : null, problem ? problemId : null]
        .filter((id): id is string => id !== null)
        .join(' ') || undefined;
      const displayValue = typeof value === 'number' || typeof value === 'string' ? String(value) : '';

      return (
        <div className="space-y-1.5">
          <label htmlFor={controlId} className="block text-ui font-medium text-[hsl(var(--text-primary))]">
            {field.label}
            {editedBadge}
          </label>
          <div className="flex items-center gap-2">
            <input
              ref={(el) => registerControl?.(fullPath, el)}
              id={controlId}
              type="number"
              min={field.min}
              step={1}
              className={`${inputClasses} max-w-[10rem]`}
              value={displayValue}
              readOnly={readOnly}
              aria-invalid={!valid || Boolean(problem) || undefined}
              aria-describedby={describedBy}
              onChange={(event) => {
                const raw = event.target.value;
                if (raw === '') {
                  onChangeValue('');
                  return;
                }
                const parsed = Number(raw);
                onChangeValue(Number.isNaN(parsed) ? raw : parsed);
              }}
            />
            <span className="text-ui text-[hsl(var(--text-secondary))]">{field.unit}</span>
          </div>
          {feedbackBlock(invalidMessage)}
        </div>
      );
    }

    case 'list': {
      const canRemove = !readOnly && draftMembers.length > field.minItems;
      const canAdd = !readOnly && (field.maxItems === null || draftMembers.length < field.maxItems);

      const updateMemberLeaf = (memberIndex: number, leafPath: string, leafValue: JsonValue) => {
        if (!onChangeList) return;
        const nextMembers = draftMembers.map((member, index) => {
          if (index !== memberIndex) return member;
          const nextValue = structuredCloneJson(member.value);
          writePointer(nextValue, leafPath, leafValue);
          return { ...member, value: nextValue };
        });
        onChangeList(nextMembers);
      };

      const toggleExclusiveLeaf = (memberIndex: number, leafPath: string) => {
        if (!onChangeList) return;
        const nextMembers = draftMembers.map((member, index) => {
          const nextValue = structuredCloneJson(member.value);
          writePointer(nextValue, leafPath, index === memberIndex);
          return { ...member, value: nextValue };
        });
        onChangeList(nextMembers);
      };

      const removeMember = (memberIndex: number) => {
        if (!onChangeList || !canRemove) return;
        onChangeList(draftMembers.filter((_, index) => index !== memberIndex));
      };

      const addMember = () => {
        if (!onChangeList || !canAdd) return;
        onChangeList([
          ...draftMembers,
          { origin: null, value: defaultMemberValue(field.item) },
        ]);
      };

      return (
        <fieldset className="space-y-3 rounded-md border border-[hsl(var(--border-subtle))] p-3">
          <legend className="px-1 text-ui font-medium text-[hsl(var(--text-primary))]">
            {field.label}
            {editedBadge}
          </legend>
          {draftMembers.map((member, memberIndex) => (
            <div
              key={`${member.origin ?? 'new'}:${memberIndex}`}
              role="group"
              aria-label={`${field.itemLabel} ${memberIndex + 1}`}
              className="space-y-2 rounded border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-sunken))] p-3"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  {field.itemLabel} {memberIndex + 1}
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  aria-label={`Remove ${field.itemLabel} ${memberIndex + 1}`}
                  disabled={!canRemove}
                  onClick={() => removeMember(memberIndex)}
                >
                  Remove
                </Button>
              </div>
              {field.item.map((leaf) => {
                const leafFullPath = joinPointer(fullPath, String(memberIndex), leaf.path);
                const leafValue = readPointer(member.value, leaf.path) ?? null;
                return (
                  <FieldControl
                    key={leaf.path}
                    field={leaf}
                    fullPath={leafFullPath}
                    value={leafValue}
                    onChangeValue={(nextValue) => updateMemberLeaf(memberIndex, leaf.path, nextValue)}
                    onExclusiveToggle={
                      leaf.type === 'flag' && leaf.exclusive
                        ? () => toggleExclusiveLeaf(memberIndex, leaf.path)
                        : undefined
                    }
                    exclusiveGroupName={
                      leaf.type === 'flag' && leaf.exclusive
                        ? `${controlId}:${leaf.path}`
                        : undefined
                    }
                    readOnly={readOnly}
                    editedPaths={editedPaths}
                    problems={problems}
                    notices={notices}
                    registerControl={registerControl}
                  />
                );
              })}
            </div>
          ))}
          <Button type="button" variant="outline" size="sm" disabled={!canAdd} onClick={addMember}>
            Add {field.itemLabel}
          </Button>
          {feedbackBlock()}
        </fieldset>
      );
    }

    default: {
      const exhaustive: never = field;
      return exhaustive;
    }
  }
}
