import { useEffect, useRef, useState } from 'react';

import { Clock, Timer } from 'lucide-react';

/** "9:05" for 545 seconds; a part of a second counts as the whole of it. */
export function formatClock(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

/**
 * Time elapsed. While `running` it counts on from `elapsedMs`; when it stops it
 * keeps what it reached, and a new `elapsedMs` sets it. Past a soft limit it says
 * so and carries on: nothing is cut off.
 */
export function ElapsedClock({
  elapsedMs,
  running,
  softLimitMs,
  label = 'Elapsed',
  pastLimit,
}: {
  elapsedMs: number;
  running: boolean;
  softLimitMs?: number | null;
  label?: string;
  pastLimit?: string;
}) {
  const [shown, setShown] = useState(elapsedMs);
  const reached = useRef(elapsedMs);

  useEffect(() => {
    reached.current = elapsedMs;
    setShown(elapsedMs);
  }, [elapsedMs]);

  useEffect(() => {
    if (!running) return;
    const base = reached.current;
    const start = Date.now();
    const timer = window.setInterval(() => {
      reached.current = base + Date.now() - start;
      setShown(reached.current);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [running, elapsedMs]);

  const limited = softLimitMs !== undefined && softLimitMs !== null;
  const past = limited && shown > softLimitMs;

  return (
    <div className="inline-flex flex-wrap items-center gap-2 text-xs">
      <p
        role="timer"
        aria-live="off"
        aria-label={label}
        className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-3 py-1 text-text-secondary"
      >
        <Clock size={13} aria-hidden="true" />
        <span className="font-semibold tabular-nums text-text-primary">{formatClock(shown)}</span>
        {limited && <span className="tabular-nums text-text-muted">/ {formatClock(softLimitMs)}</span>}
      </p>
      {past && (
        <span
          role="status"
          className="inline-flex items-center gap-1 rounded-full bg-warning-muted px-2.5 py-0.5 font-medium text-warning-fg"
        >
          <Timer size={13} aria-hidden="true" />
          {pastLimit ?? `Past ${formatClock(softLimitMs)}. You can keep going.`}
        </span>
      )}
    </div>
  );
}
