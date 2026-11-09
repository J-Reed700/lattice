import type * as Wire from '@/lib/bindings';

export interface NewAttemptProps {
  choice: Wire.LearningPracticeMode;
  onChoose: (mode: Wire.LearningPracticeMode) => void;
  onStart: () => void;
  starting: boolean;
  busy: boolean;
}

export function NewAttempt(_props: NewAttemptProps) {
  return null;
}
