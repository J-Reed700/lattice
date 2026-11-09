import { Check } from 'lucide-react';
import type { CodingStage } from './codingApi';

export interface StageRailProps {
  currentStage: CodingStage;
  onSelectStage?: (stage: CodingStage) => void;
  className?: string;
}

const STAGES: { stage: CodingStage; label: string; stepNumber: number }[] = [
  { stage: 'understand', label: 'Understand', stepNumber: 1 },
  { stage: 'plan', label: 'Plan', stepNumber: 2 },
  { stage: 'code', label: 'Code', stepNumber: 3 },
  { stage: 'verify', label: 'Verify', stepNumber: 4 },
];

export function StageRail({
  currentStage,
  onSelectStage,
  className = '',
}: StageRailProps) {
  const currentIndex = STAGES.findIndex((s) => s.stage === currentStage);

  return (
    <nav aria-label="Stages" className={className}>
      <ol className="flex flex-wrap items-center gap-1.5 list-none m-0 p-0">
        {STAGES.map((s, index) => {
          const isCurrent = s.stage === currentStage;
          const isPast = index < currentIndex;

          return (
            <li key={s.stage}>
              <button
                type="button"
                onClick={() => onSelectStage?.(s.stage)}
                aria-current={isCurrent ? 'step' : undefined}
                className={`inline-flex min-h-[44px] items-center gap-2 rounded-lg px-3.5 text-sm transition-colors cursor-pointer ${
                  isCurrent
                    ? 'bg-accent-muted text-accent font-semibold'
                    : isPast
                    ? 'text-text-secondary hover:text-text-primary font-medium'
                    : 'text-text-tertiary hover:text-text-secondary font-medium'
                }`}
              >
                {isPast ? (
                  <Check className="h-4 w-4 text-success-fg" aria-hidden="true" />
                ) : (
                  <span
                    aria-hidden="true"
                    className={`grid h-5 w-5 place-items-center rounded-full text-xs font-mono ${
                      isCurrent
                        ? 'bg-accent text-accent-fg'
                        : 'bg-surface-raised text-text-tertiary'
                    }`}
                  >
                    {s.stepNumber}
                  </span>
                )}
                <span>{s.label}</span>
              </button>
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
