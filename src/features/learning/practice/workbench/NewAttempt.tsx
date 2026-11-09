import type * as Wire from '@/lib/bindings';

import { HELP_MODES } from './helpModes';

export interface NewAttemptProps {
  choice: Wire.LearningPracticeMode;
  onChoose: (mode: Wire.LearningPracticeMode) => void;
  onStart: () => void;
  starting: boolean;
  busy: boolean;
}

export function NewAttempt({ choice, onChoose, onStart, starting, busy }: NewAttemptProps) {
  return (
    <div className="mt-5 grid gap-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-start">
      <fieldset className="min-w-0">
        <legend className="text-xs font-semibold text-text-primary">Help</legend>
        <div className="mt-2 grid gap-2 sm:grid-cols-3">
          {HELP_MODES.map((item) => (
            <label
              key={item.id}
              className={`cursor-pointer rounded-xl border p-3 transition ${
                choice === item.id ? 'border-accent bg-accent-muted' : 'border-border-default bg-surface'
              }`}
            >
              <span className="flex items-center gap-2">
                <input
                  type="radio"
                  name="new-session-mode"
                  value={item.id}
                  checked={choice === item.id}
                  onChange={() => onChoose(item.id)}
                  className="accent-accent"
                />
                <span className="text-xs font-semibold text-text-primary">{item.help}</span>
              </span>
              <span className="mt-1 block text-xs leading-5 text-text-secondary">{item.description}</span>
            </label>
          ))}
        </div>
      </fieldset>
      <div>
        <button
          type="button"
          disabled={busy}
          onClick={onStart}
          className="rounded-full bg-accent px-5 py-3 text-xs font-semibold text-accent-fg disabled:opacity-50"
        >
          {starting ? 'Starting…' : 'Start an attempt'}
        </button>
        {busy && !starting && <p className="mt-1.5 text-xs text-text-muted">Another action is in progress.</p>}
      </div>
    </div>
  );
}
