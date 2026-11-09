import { useState, type FormEvent } from 'react';
import { Play, Send } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

export interface RunBarProps {
  signature: Wire.ProblemSignature;
  running: boolean;
  onRunExamples: () => void;
  onSubmit: () => void;
  onTryInput: (args: string[]) => void;
}

const SECONDARY =
  'inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50';

export function RunBar({ signature, running, onRunExamples, onSubmit, onTryInput }: RunBarProps) {
  const [open, setOpen] = useState(false);
  const [args, setArgs] = useState<string[]>(signature.params.map(() => ''));
  const complete = args.every((value) => value.trim() !== '');

  const submitInput = (event: FormEvent) => {
    event.preventDefault();
    if (complete && !running) onTryInput(args.map((value) => value.trim()));
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-start gap-2.5">
        <button
          type="button"
          onClick={onRunExamples}
          disabled={running}
          className="inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Play className="h-4 w-4" aria-hidden="true" />
          <span>Run examples</span>
          <kbd className="rounded border border-accent-fg/40 px-1.5 py-0.5 font-mono text-xs">Ctrl+Enter</kbd>
        </button>
        <button type="button" onClick={() => setOpen((value) => !value)} aria-expanded={open} className={SECONDARY}>
          Try an input
        </button>
        <button type="button" onClick={onSubmit} disabled={running} className={SECONDARY}>
          <Send className="h-4 w-4" aria-hidden="true" />
          <span>Submit</span>
          <kbd className="rounded border border-border-default px-1.5 py-0.5 font-mono text-xs text-text-secondary">
            Ctrl+Shift+Enter
          </kbd>
        </button>
      </div>
      {running ? (
        <p role="status" className="m-0 text-sm text-text-secondary">
          Running your code. You can keep editing.
        </p>
      ) : null}

      {open ? (
        <form
          onSubmit={submitInput}
          aria-label="Try an input"
          className="flex flex-wrap items-end gap-3 rounded-lg border border-border-default bg-surface-sunken p-3"
        >
          {signature.params.map((param, index) => (
            <div key={param.name} className="flex min-w-[8rem] flex-1 flex-col gap-1">
              <label htmlFor={`coding-input-${param.name}`} className="text-xs font-semibold text-text-primary">
                {param.name}
              </label>
              <input
                id={`coding-input-${param.name}`}
                value={args[index] ?? ''}
                onChange={(event) =>
                  setArgs((current) => current.map((value, at) => (at === index ? event.target.value : value)))
                }
                spellCheck={false}
                autoComplete="off"
                className="min-h-[40px] rounded-lg border border-border-default bg-surface px-3 py-1.5 font-mono text-sm text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              />
            </div>
          ))}
          <div className="flex flex-col gap-1">
            <button
              type="submit"
              disabled={!complete || running}
              aria-describedby={!complete ? 'coding-input-reason' : undefined}
              className="min-h-[40px] cursor-pointer rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Run input
            </button>
            {!complete ? (
              <span id="coding-input-reason" className="text-xs text-text-secondary">
                Fill in every argument, as Python values.
              </span>
            ) : null}
          </div>
        </form>
      ) : null}
    </div>
  );
}
