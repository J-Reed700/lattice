import type { LearningPracticeProfileDto } from './practiceHomeApi';

export interface PreparingForProps {
  programId: string;
  courseGoal?: string;
  profile?: LearningPracticeProfileDto | null;
  onProfileChange?: (profile: LearningPracticeProfileDto) => void;
  className?: string;
}

/** Stage 1 minimal non-panicking stub: returns null. */
export function PreparingFor(_props: PreparingForProps): JSX.Element | null {
  return null;
}
