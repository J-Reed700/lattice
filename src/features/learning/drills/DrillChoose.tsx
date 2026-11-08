import type { DrillOptions } from './drillOptions';

/** What the learner picks before a drill: the lesson, what to review, how they practise it and how long it is. */
export function DrillChoose(_props: {
  programId: string;
  options: DrillOptions;
  onChange: (patch: Partial<DrillOptions>) => void;
  onStart: (patch: Partial<DrillOptions>) => void;
  onOpenAttempt: (attemptId: string) => void;
}) {
  return null;
}
