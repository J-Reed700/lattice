export interface ElapsedClockProps {
  elapsedMs?: number;
  startedAt?: number | string | null;
  running?: boolean;
  budgetMs?: number | null;
  className?: string;
}

export function formatClock(_ms: number): string {
  return '';
}

export function ElapsedClock(_props: ElapsedClockProps) {
  return <></>;
}
