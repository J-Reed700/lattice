import { ArrowRight, Check } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

export interface StageRailProps {
  stages: Wire.StageLabelDto[];
  current: Wire.Stage;
  /** What the kind says to do at the current stage. */
  instruction: string;
  busy?: boolean;
  onSelect: (stage: Wire.Stage) => void;
}

export function StageRail({ stages, current, instruction, busy = false, onSelect }: StageRailProps) {
  const index = stages.findIndex((item) => item.stage === current);
  const next = stages[index + 1];

  return (
    <nav aria-label="Stages" className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <ol className="m-0 flex list-none flex-wrap items-center gap-1.5 p-0">
          {stages.map(({ stage, label }, at) => {
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
                  <span>{label}</span>
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
            onClick={() => onSelect(next.stage)}
            className="inline-flex min-h-[44px] cursor-pointer items-center gap-2 rounded-lg border border-border-default bg-surface px-3 text-sm font-semibold text-text-primary hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-60"
          >
            <span>Next: {next.label}</span>
            <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </button>
        ) : (
          <span className="text-sm text-text-secondary">This is the last stage. Finish when you are done.</span>
        )}
      </div>
      <p className="m-0 text-sm text-text-secondary">{instruction} Any stage can be skipped.</p>
    </nav>
  );
}
