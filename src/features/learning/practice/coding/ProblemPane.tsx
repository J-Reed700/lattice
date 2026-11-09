import type { PublicProblemDto, Setting } from './codingApi';

export interface ProblemPaneProps {
  problem: PublicProblemDto;
  setting: Setting;
  className?: string;
}

/** ProblemPane - problem statement, function signature, constraints, subgoals, and worked solution. Minimal non-crashing stub for Stage 1. */
export function ProblemPane(_props: ProblemPaneProps) {
  return <section aria-label="Problem" data-testid="stub-problem-pane" />;
}
