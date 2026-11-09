import type { DueItemDto } from './practiceHomeApi';

export interface DueStripProps {
  programId: string;
  items?: DueItemDto[];
  onStartMostOverdue?: () => void;
  className?: string;
}

/** Stage 1 minimal non-panicking stub: returns null. */
export function DueStrip(_props: DueStripProps) {
  return null;
}
