import type { CodingStage } from './codingApi';

export interface StageRailProps {
  currentStage: CodingStage;
  onSelectStage?: (stage: CodingStage) => void;
  className?: string;
}

/** StageRail - 4-stage rail (Understand -> Plan -> Code -> Verify). Minimal non-crashing stub for Stage 1. */
export function StageRail(_props: StageRailProps) {
  return <nav aria-label="Stages" data-testid="stub-stage-rail" />;
}
