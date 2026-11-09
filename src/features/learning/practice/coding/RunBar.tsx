import { useState } from 'react';
import { Play } from 'lucide-react';
import type { LearningPracticalRunDto } from '@/lib/bindings';
import { CaseResults, rowsFromChecks } from '../shared/CaseResults';

export interface RunBarProps {
  onRunExamples: () => void;
  onRunCustom: (customInput: string) => void;
  onSubmit: () => void;
  running?: boolean;
  lastRun?: LearningPracticalRunDto | null;
  className?: string;
}

export function RunBar({
  onRunExamples,
  onRunCustom,
  onSubmit,
  running = false,
  lastRun,
  className = '',
}: RunBarProps) {
  const [showCustomInput, setShowCustomInput] = useState(false);
  const [customInput, setCustomInput] = useState('');

  const handleCustomSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!customInput.trim() || running) return;
    onRunCustom(customInput);
  };

  const caseRows = lastRun?.checks ? rowsFromChecks(lastRun.checks) : [];
  const passedCount = caseRows.filter((r) => r.outcome === 'passed').length;
  const totalCases = caseRows.length;
  const failingCase = caseRows.find((r) => r.outcome === 'failed' || r.outcome === 'error');
  const summaryText = totalCases > 0
    ? `${passedCount} of ${totalCases} passed.${failingCase ? ` Failing: ${failingCase.label}` : ''}`
    : undefined;

  return (
    <div className={`space-y-4 ${className}`}>
      {/* Button bar */}
      <div className="flex flex-wrap items-center gap-2.5">
        <button
          type="button"
          onClick={onRunExamples}
          disabled={running}
          className="inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Play className="h-4 w-4" aria-hidden="true" />
          <span>Run examples</span>
          <kbd className="ml-1 rounded border border-accent-fg/40 px-1.5 py-0.5 font-mono text-xxs font-medium text-accent-fg">
            Ctrl+Enter
          </kbd>
        </button>

        <button
          type="button"
          onClick={() => setShowCustomInput((v) => !v)}
          disabled={running}
          className="inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary transition-colors hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50"
        >
          Try an input
        </button>

        <button
          type="button"
          onClick={onSubmit}
          disabled={running}
          className="inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary transition-colors hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50"
        >
          <span>Submit</span>
          <kbd className="ml-1 rounded border border-border-default px-1.5 py-0.5 font-mono text-xxs font-medium text-text-secondary">
            Ctrl+Shift+Enter
          </kbd>
        </button>
      </div>

      {/* Collapsible custom input editor */}
      {showCustomInput && (
        <form onSubmit={handleCustomSubmit} className="flex flex-wrap items-end gap-2.5 rounded-lg border border-border-default bg-surface-sunken p-3">
          <div className="flex flex-col gap-1.5 flex-1 min-w-[200px]">
            <label htmlFor="custom-arg-input" className="text-xs font-semibold text-text-primary">
              Arguments (Python literal)
            </label>
            <input
              id="custom-arg-input"
              type="text"
              value={customInput}
              onChange={(e) => setCustomInput(e.target.value)}
              placeholder='e.g. "a b c d"'
              className="min-h-[40px] rounded-lg border border-border-default bg-surface px-3 py-1.5 font-mono text-xs text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            />
          </div>
          <button
            type="submit"
            disabled={running || !customInput.trim()}
            className="inline-flex min-h-[40px] cursor-pointer items-center justify-center rounded-lg bg-accent px-4 text-xs font-semibold text-accent-fg hover:opacity-90 disabled:opacity-50"
          >
            Run input
          </button>
        </form>
      )}

      {/* Case results from last run */}
      {lastRun && (
        <CaseResults
          rows={caseRows}
          summary={summaryText}
          running={running}
          output={
            lastRun.stdout || lastRun.stderr
              ? { stdout: lastRun.stdout, stderr: lastRun.stderr, truncated: lastRun.outputTruncated }
              : undefined
          }
        />
      )}
    </div>
  );
}
