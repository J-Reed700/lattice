import type * as Wire from '@/lib/bindings';

export interface CodingBuildChoice {
  lessonId?: string | null;
  task: Wire.CodingTask;
  setting: Wire.CodingSetting;
  language: string;
  difficulty: Wire.ProblemDifficulty;
  focus?: string | null;
  budgetSeconds?: number | null;
}

export interface CodingEntryProps {
  programId: string;
  choice: CodingBuildChoice;
  onReady: (sessionId: string) => void;
  onCancel: () => void;
}

export function CodingEntry(_props: CodingEntryProps): JSX.Element | null {
  return null;
}
