import type { LearningPracticalRunDto } from '@/lib/bindings';

export interface RunBarProps {
  onRunExamples: () => void;
  onRunCustom: (customInput: string) => void;
  onSubmit: () => void;
  running?: boolean;
  lastRun?: LearningPracticalRunDto | null;
  className?: string;
}

/** RunBar - Run examples, Try an input, Submit, and CaseResults. Minimal non-crashing stub for Stage 1. */
export function RunBar(_props: RunBarProps) {
  return <div data-testid="stub-run-bar" />;
}
