import type { CodingDebriefDto } from './codingApi';

export interface DebriefProps {
  debrief: CodingDebriefDto;
  programId: string;
  lessonId?: string | null;
  onRetryInterview?: () => void;
  onExit?: () => void;
  className?: string;
}

/** Debrief - session debrief screen with criteria scorecard, patterns, timings, and strong answer. Minimal non-crashing stub for Stage 1. */
export function Debrief(_props: DebriefProps) {
  return <div data-testid="stub-debrief" />;
}
