import type { CheckedCodeHint, CodeHintRung } from './codingApi';

export interface HintLadderProps {
  hints: CheckedCodeHint[];
  availableNextRung?: CodeHintRung | null;
  onAskHint: () => void;
  asking?: boolean;
  className?: string;
}

/** HintLadder - Coached-only 4-rung progressive hint ladder. Minimal non-crashing stub for Stage 1. */
export function HintLadder(_props: HintLadderProps) {
  return <section aria-label="Hints" data-testid="stub-hint-ladder" />;
}
