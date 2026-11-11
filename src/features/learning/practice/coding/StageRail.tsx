import { ArrowRight, Check } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

export const STAGE_NAMES: Record<Wire.CodingStage, string> = {
  understand: 'Understand',
  examples: 'Examples',
  plan: 'Plan',
  code: 'Code',
  test: 'Test',
  review: 'Review',
  reproduce: 'Reproduce',
  locate: 'Locate',
  fix: 'Fix',
  verify: 'Verify',
};

const INSTRUCTIONS: Record<Wire.CodingStage, string> = {
  understand: 'Read the statement and ask about anything it leaves open.',
  examples: 'Write examples with the answers you expect, before any code.',
  plan: 'Say how you will solve it and what that costs.',
  code: 'Write the function.',
  test: 'Run your examples, then submit to run every case.',
  review: 'Explain the cost and what you would change.',
  reproduce: 'Make the bug happen with an input you choose.',
  locate: 'Find the line where the result goes wrong.',
  fix: 'Change the code until the failing inputs pass.',
  verify: 'Submit to run every case and confirm nothing else broke.',
};

export interface StageRailProps {
  stages: Wire.CodingStage[];
  current: Wire.CodingStage;
  busy?: boolean;
  onSelect: (stage: Wire.CodingStage) => void;
}

export function StageRail({ stages, current, busy = false, onSelect }: StageRailProps) {
  const index = stages.indexOf(current);
  const next = stages[index + 1];

  return (
    <nav aria-label="Stages" className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <ol className="m-0 flex list-none flex-wrap items-center gap-1.5 p-0">
          {stages.map((stage, at) => {
            const isCurrent = stage === current;
            const done = at < index;
            return (
              <li key={stage}>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onSelect(stage)}
                  aria-current={isCurrent ? 'step' : undefined}
                  className={`inline-flex min-h-[44px] cursor-pointer items-center gap-2 rounded-lg px-3 text-sm disabled:cursor-not-allowed disabled:opacity-60 ${
                    isCurrent
                      ? 'bg-accent-muted font-semibold text-accent'
                      : 'font-medium text-text-secondary hover:text-text-primary'
                  }`}
                >
                  {done ? (
                    <Check className="h-4 w-4 text-success-fg" aria-hidden="true" />
                  ) : (
                    <span
                      aria-hidden="true"
                      className={`grid h-5 w-5 place-items-center rounded-full font-mono text-xs ${
                        isCurrent ? 'bg-accent text-accent-fg' : 'bg-surface-raised text-text-secondary'
                      }`}
                    >
                      {at + 1}
                    </span>
                  )}
                  <span>{STAGE_NAMES[stage]}</span>
                  {done ? <span className="sr-only">(done)</span> : null}
                </button>
              </li>
            );
          })}
        </ol>
        {next ? (
          <button
            type="button"
            disabled={busy}
            onClick={() => onSelect(next)}
            className="inline-flex min-h-[44px] cursor-pointer items-center gap-2 rounded-lg border border-border-default bg-surface px-3 text-sm font-semibold text-text-primary hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-60"
          >
            <span>Next: {STAGE_NAMES[next]}</span>
            <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </button>
        ) : (
          <span className="text-sm text-text-secondary">This is the last stage. Finish when you are done.</span>
        )}
      </div>
      <p className="m-0 text-sm text-text-secondary">{INSTRUCTIONS[current]} Any stage can be skipped.</p>
    </nav>
  );
}
