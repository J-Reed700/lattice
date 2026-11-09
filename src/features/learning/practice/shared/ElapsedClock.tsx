export function formatClock(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

export function ElapsedClock(_props: {
  elapsedMs: number;
  running: boolean;
  softLimitMs?: number | null;
  label?: string;
}) {
  return null;
}
