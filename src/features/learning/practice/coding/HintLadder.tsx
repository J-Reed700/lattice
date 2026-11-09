import { Check } from 'lucide-react';
import type { CheckedCodeHint, CodeHintRung } from './codingApi';

export interface HintLadderProps {
  hints: CheckedCodeHint[];
  availableNextRung?: CodeHintRung | null;
  onAskHint: () => void;
  asking?: boolean;
  className?: string;
}

const RUNGS: { rung: CodeHintRung; n: number; name: string; label: string }[] = [
  { rung: 'nudge', n: 1, name: 'Nudge', label: 'Get a nudge' },
  { rung: 'idea', n: 2, name: 'Idea', label: 'Get the idea' },
  { rung: 'plan', n: 3, name: 'Plan', label: 'Get a plan' },
  { rung: 'point_at_line', n: 4, name: 'Point at the line', label: 'Point at the line' },
];

export function HintLadder({
  hints,
  availableNextRung,
  onAskHint,
  asking = false,
  className = '',
}: HintLadderProps) {
  const hintsByRung = new Map<CodeHintRung, CheckedCodeHint>();
  for (const h of hints) {
    hintsByRung.set(h.rung, h);
  }

  return (
    <section
      aria-labelledby="hint-ladder-heading"
      className={`rounded-xl border border-border-default bg-surface p-5 shadow-xs ${className}`}
    >
      <div className="border-b border-border-subtle pb-3 mb-4">
        <h3
          id="hint-ladder-heading"
          className="font-serif text-lg font-semibold text-text-primary m-0"
        >
          Stuck?
        </h3>
        <p className="mt-1 text-xs text-text-tertiary m-0">
          One at a time. None of them gives code.
        </p>
      </div>

      <div className="space-y-4">
        {RUNGS.map((item) => {
          const usedHint = hintsByRung.get(item.rung);
          const isUsed = Boolean(usedHint);
          const isNextAvailable = availableNextRung === item.rung && !isUsed;
          const isLocked = !isUsed && !isNextAvailable;

          return (
            <div
              key={item.rung}
              className="rounded-lg border border-border-subtle bg-surface-sunken/40 p-3 space-y-2"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-medium text-sm text-text-primary">
                  <span className="mr-2 font-mono text-text-tertiary">
                    {item.n}
                  </span>
                  {item.name}
                </span>

                {isUsed ? (
                  <span className="inline-flex min-h-[28px] items-center gap-1.5 rounded-full bg-success-muted px-2.5 text-xs font-medium text-success-fg">
                    <Check className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>Used</span>
                  </span>
                ) : (
                  <div className="flex flex-col items-end gap-1">
                    <button
                      type="button"
                      disabled={isLocked || asking}
                      onClick={onAskHint}
                      className="inline-flex min-h-[36px] cursor-pointer items-center justify-center rounded-lg border border-border-default bg-surface px-3 text-xs font-semibold text-text-primary transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      {asking && isNextAvailable ? 'Getting hint…' : item.label}
                    </button>
                    {isLocked && (
                      <span className="text-xxs text-text-tertiary">
                        Opens after the one before
                      </span>
                    )}
                  </div>
                )}
              </div>

              {/* Revealed hint content and reason */}
              {usedHint && (
                <div className="space-y-1.5 pt-1 text-xs">
                  <p className="text-text-primary font-medium m-0">
                    {usedHint.content}
                  </p>
                  {usedHint.reason && (
                    <p className="text-text-tertiary italic m-0">
                      Why: {usedHint.reason}
                    </p>
                  )}
                  {usedHint.lines && usedHint.lines.length > 0 && (
                    <p className="font-mono text-danger-fg m-0">
                      Check lines: {usedHint.lines.join(', ')}
                    </p>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}
