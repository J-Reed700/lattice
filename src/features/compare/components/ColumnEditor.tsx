import { useRef, useState } from 'react';

import { Plus, X } from 'lucide-react';

import { Button } from '@/components/ui/button';

const PRESETS = [
  'method, sample size, finding',
  'prep time, servings, main protein',
  'parties, term, termination',
];

interface ColumnEditorProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (columns: string[]) => void;
  isRunning: boolean;
}

/** Comma-separated plain language, trimmed, deduped case-insensitively. */
export function parseColumns(value: string): string[] {
  const seen = new Set<string>();
  const columns: string[] = [];
  for (const part of value.split(',')) {
    const trimmed = part.trim();
    if (!trimmed) continue;
    const key = trimmed.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    columns.push(trimmed);
  }
  return columns;
}

function mergeColumns(existing: string[], incomingRaw: string): string[] {
  return parseColumns([...existing, incomingRaw].join(', '));
}

/**
 * Interactive removable column chips + inline add-column input, three presets,
 * and a primary `--action` Compare button. Full keyboard ergonomics:
 * Enter / comma adds a chip, Enter on empty input submits, Backspace removes
 * the last chip.
 */
export function ColumnEditor({ value, onChange, onSubmit, isRunning }: ColumnEditorProps) {
  const [draft, setDraft] = useState('');
  const [touched, setTouched] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const columns = parseColumns(value);
  const effectiveColumns = draft.trim() ? mergeColumns(columns, draft) : columns;

  const hint =
    effectiveColumns.length > 0
      ? `${effectiveColumns.length} ${effectiveColumns.length === 1 ? 'column' : 'columns'}`
      : touched
        ? 'Name at least one column.'
        : '';

  const commitDraft = (raw: string) => {
    const next = mergeColumns(columns, raw);
    if (next.length !== columns.length || next.some((col, idx) => col !== columns[idx])) {
      onChange(next.join(', '));
    }
    setDraft('');
  };

  const removeColumn = (target: string) => {
    const next = columns.filter((col) => col !== target);
    onChange(next.join(', '));
    inputRef.current?.focus();
  };

  const submit = () => {
    setTouched(true);
    const next = draft.trim() ? mergeColumns(columns, draft) : columns;
    if (draft.trim()) {
      onChange(next.join(', '));
      setDraft('');
    }
    if (next.length === 0 || isRunning) return;
    onSubmit(next);
  };

  return (
    <div className="mb-6 rounded-xl border border-border-subtle bg-surface p-5 shadow-sm">
      <div className="flex items-baseline justify-between gap-2">
        <label
          htmlFor="compare-columns"
          className="block font-sans text-xs font-semibold tracking-[-0.01em] text-text-secondary"
        >
          Columns
        </label>
        {hint ? (
          <p className="text-xs text-text-muted tabular-nums">{hint}</p>
        ) : null}
      </div>

      <div className="mt-2 flex flex-wrap items-start gap-2.5 sm:flex-nowrap">
        <div
          onClick={() => inputRef.current?.focus()}
          className="flex min-h-9 min-w-0 flex-1 cursor-text flex-wrap items-center gap-1.5 rounded-lg border border-border-default bg-surface px-2 py-1.5 shadow-control transition-[border-color,box-shadow] duration-fast hover:border-border-strong focus-within:border-accent/60 focus-within:ring-2 focus-within:ring-ring"
        >
          {columns.map((col) => (
            <span
              key={col}
              className="inline-flex h-6 items-center gap-1 rounded-md bg-[hsl(var(--text-primary)/0.07)] pl-2 pr-1 text-xs font-medium text-text-primary"
            >
              <span>{col}</span>
              <button
                type="button"
                disabled={isRunning}
                aria-label={`Remove column ${col}`}
                onClick={(event) => {
                  event.stopPropagation();
                  removeColumn(col);
                }}
                className="flex h-4 w-4 items-center justify-center rounded-sm text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.1)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-40"
              >
                <X className="h-3 w-3" strokeWidth={2} aria-hidden="true" />
              </button>
            </span>
          ))}

          <input
            ref={inputRef}
            id="compare-columns"
            type="text"
            value={draft}
            disabled={isRunning}
            onChange={(event) => {
              const raw = event.target.value;
              if (raw.includes(',')) {
                const parts = raw.split(',');
                const tail = parts.pop() ?? '';
                const committed = parts.join(',');
                if (committed.trim()) {
                  const next = mergeColumns(columns, committed);
                  onChange(next.join(', '));
                }
                setDraft(tail);
              } else {
                setDraft(raw);
              }
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                if (draft.trim()) {
                  commitDraft(draft);
                } else {
                  submit();
                }
              } else if (event.key === 'Backspace' && draft === '' && columns.length > 0) {
                event.preventDefault();
                const last = columns[columns.length - 1];
                if (last) removeColumn(last);
              }
            }}
            placeholder={columns.length === 0 ? 'Add columns (e.g. method, sample size, finding)' : 'Add column…'}
            className="h-6 min-w-[140px] flex-1 bg-transparent px-1 text-ui text-text-primary outline-hidden placeholder:text-text-muted disabled:opacity-50"
          />

          {draft.trim().length > 0 && (
            <button
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                commitDraft(draft);
                inputRef.current?.focus();
              }}
              aria-label="Add column"
              className="inline-flex h-6 items-center gap-1 rounded-md bg-[hsl(var(--text-primary)/0.07)] px-2 text-xs font-medium text-text-primary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.12)] focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-ring"
            >
              <Plus className="h-3 w-3" strokeWidth={2} aria-hidden="true" />
              Add
            </button>
          )}
        </div>

        <Button
          type="button"
          onClick={submit}
          disabled={isRunning || effectiveColumns.length === 0}
          className="h-9 shrink-0 px-4"
        >
          Compare
        </Button>
      </div>

      <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
        <span className="mr-1 text-xs text-text-muted">Presets:</span>
        {PRESETS.map((preset) => (
          <button
            key={preset}
            type="button"
            disabled={isRunning}
            onClick={() => {
              setDraft('');
              onChange(preset);
            }}
            className="inline-flex h-6 items-center rounded-full border border-border-subtle bg-surface px-2.5 text-xs text-text-secondary transition-colors duration-fast hover:border-border-default hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          >
            {preset}
          </button>
        ))}
      </div>
    </div>
  );
}
