import { useRef, type MouseEvent, type ReactElement, type ReactNode } from 'react';

import { Pencil } from 'lucide-react';

import { useEditMode } from './editMode';
import { openEditor } from './editorStore';
import './edits.css';

import type { ContentRef } from './types';

export interface EditableProps {
  content: ContentRef;
  label: string;
  path?: string;
  source?: string;
  sourceStart?: number;
  className?: string;
  children: ReactNode;
}

const EXCLUDED_TARGETS =
  'a, button, input, textarea, select, summary, label, [role="button"], [data-cite], [data-code-ref]';

function parseLineRange(raw: string | null): [number, number] | null {
  if (!raw) return null;
  const match = /^(\d+)-(\d+)$/.exec(raw);
  if (!match) return null;
  return [Number(match[1]), Number(match[2])];
}

export function Editable({
  content,
  label,
  path,
  source,
  sourceStart,
  className,
  children,
}: EditableProps): ReactElement {
  const on = useEditMode((state) => state.on);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const handleClickCapture = (event: MouseEvent<HTMLDivElement>) => {
    if (source === undefined) return;
    const rawTarget = event.target;
    const targetEl = rawTarget instanceof Element ? rawTarget : rawTarget instanceof Node ? rawTarget.parentElement : null;
    if (!targetEl) return;
    if (targetEl.closest('[data-editable-kind]') !== event.currentTarget) return;
    if (targetEl.closest(EXCLUDED_TARGETS)) return;
    if ((window.getSelection()?.toString() ?? '') !== '') return;

    const lineEl = targetEl.closest('[data-source-lines]');
    if (!lineEl || !event.currentTarget.contains(lineEl)) return;
    const lines = parseLineRange(lineEl.getAttribute('data-source-lines'));
    if (!lines) return;

    event.stopPropagation();
    openEditor(
      { content, path: path ?? null, fragment: null },
      { aim: { lines, source, sourceStart: sourceStart ?? 0 }, returnFocusTo: buttonRef.current },
    );
  };

  const wrapperClass = on ? (className ? `editable-piece ${className}` : 'editable-piece') : 'contents';

  return (
    <div
      className={wrapperClass}
      data-editable-kind={on ? content.kind : undefined}
      onClickCapture={on ? handleClickCapture : undefined}
    >
      {on ? (
        <button
          ref={buttonRef}
          type="button"
          className="editable-piece-button inline-flex items-center gap-1 rounded px-2 py-0.5 text-xs font-medium text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]"
          aria-label={`Edit ${label}`}
          onClick={(event) => {
            event.stopPropagation();
            openEditor({ content, path: null, fragment: null }, { returnFocusTo: event.currentTarget });
          }}
        >
          <Pencil className="h-3.5 w-3.5" aria-hidden="true" />
          Edit
        </button>
      ) : null}
      {children}
    </div>
  );
}
