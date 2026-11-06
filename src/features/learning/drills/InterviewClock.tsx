import { useEffect, useState } from 'react';

import { Timer } from 'lucide-react';

/** "9:05" for 545 seconds; a part of a second counts as the whole of it. */
export function formatClock(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

/**
 * The interview's time left, counted from the moment the attempt started. At zero it says so and goes on: the
 * review is the learner's to hand in.
 */
export function InterviewClock({ startedAt, limitMs }: { startedAt: number; limitMs: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const left = limitMs - (now - startedAt);
  if (left <= 0) {
    return (
      <p role="status" className="inline-flex items-center gap-1.5 rounded-full bg-warning-muted px-3 py-1 text-xs font-semibold text-warning-fg">
        <Timer size={13} aria-hidden="true" />
        Time is up. You can keep going or hand in your review now.
      </p>
    );
  }
  return (
    <p role="timer" aria-live="off" className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-3 py-1 text-xs text-text-secondary">
      <Timer size={13} aria-hidden="true" />
      <span>
        Time left <span className="font-semibold tabular-nums text-text-primary">{formatClock(left)}</span>
      </span>
    </p>
  );
}
