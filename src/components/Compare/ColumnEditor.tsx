import { useState } from 'react';

const MAX_COLUMNS = 6;

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

/**
 * One text input, three presets, one submit. No card — the editor sits under
 * the page header on a hairline.
 */
export function ColumnEditor({ value, onChange, onSubmit, isRunning }: ColumnEditorProps) {
  const [touched, setTouched] = useState(false);
  const parsed = parseColumns(value);
  const overLimit = parsed.length > MAX_COLUMNS;
  const columns = parsed.slice(0, MAX_COLUMNS);

  const hint = overLimit
    ? 'Six columns maximum.'
    : columns.length > 0
      ? `${columns.length} of ${MAX_COLUMNS} columns`
      : touched
        ? 'Name at least one column.'
        : '';

  const submit = () => {
    setTouched(true);
    if (columns.length === 0 || isRunning) return;
    onSubmit(columns);
  };

  return (
    <div className="mb-8 border-t border-border-subtle pt-6">
      <label
        htmlFor="compare-columns"
        className="block text-sm font-medium text-text-secondary"
      >
        Columns
      </label>
      <div className="mt-2 flex items-center gap-3">
        <input
          id="compare-columns"
          type="text"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              submit();
            }
          }}
          placeholder="method, sample size, finding"
          className="h-9 min-w-0 flex-1 rounded-sm border border-border-default bg-surface px-2.5 text-sm text-text-primary outline-none transition-colors duration-fast placeholder:text-text-muted focus:border-accent"
        />
        <button
          type="button"
          onClick={submit}
          disabled={isRunning || columns.length === 0}
          className="h-9 shrink-0 rounded-sm bg-[hsl(var(--accent))] px-3 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:opacity-50"
        >
          Compare
        </button>
      </div>

      <div className="mt-2 flex flex-wrap items-center gap-4">
        {PRESETS.map((preset) => (
          <button
            key={preset}
            type="button"
            onClick={() => onChange(preset)}
            className="text-xs text-text-muted transition-colors duration-fast hover:text-text-secondary"
          >
            {preset}
          </button>
        ))}
      </div>

      {hint ? (
        <p className="mt-2 text-xs text-text-muted tabular-nums">{hint}</p>
      ) : null}
    </div>
  );
}
