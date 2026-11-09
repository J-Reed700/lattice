import type { CodingSessionDto } from './codingApi';

export interface CodingSessionProps {
  session: CodingSessionDto;
  onUpdateSession?: (session: CodingSessionDto) => void;
  onFinish?: (session: CodingSessionDto) => void;
  onExit?: () => void;
  className?: string;
}

/** CodingSession - 3-column workspace with editor, runner, stages, and thread. Minimal non-crashing stub for Stage 1. */
export function CodingSession(_props: CodingSessionProps) {
  return <div data-testid="stub-coding-session" />;
}
