import { useEffect, useRef, useState } from 'react';

/**
 * What the turn is doing, while it is doing it.
 *
 * A tool round produces no text at all until the model has finished reasoning
 * and written its tool calls. On a slow or remote model that is minutes of an
 * unchanging spinner, which reads as a hang — so this says what phase the turn
 * is in and how long it has been there. The clock is the point: a label alone
 * cannot distinguish "started this a second ago" from "stuck here for six
 * minutes".
 */
export function ActivityNote({ detail }: { detail: string | null }) {
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  // Each new phase restarts the clock, so the number always describes the phase
  // named beside it rather than the whole turn.
  const startedAt = useRef(Date.now());
  const previousDetail = useRef(detail);

  if (previousDetail.current !== detail) {
    previousDetail.current = detail;
    startedAt.current = Date.now();
  }

  useEffect(() => {
    if (!detail) return;
    startedAt.current = Date.now();
    setElapsedSeconds(0);
    const timer = setInterval(() => {
      setElapsedSeconds(Math.floor((Date.now() - startedAt.current) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [detail]);

  if (!detail) return null;

  return (
    <p
      className="mb-2 text-xs text-[hsl(var(--text-tertiary))]"
      role="status"
      aria-live="polite"
    >
      {detail}
      {elapsedSeconds >= 3 && <> · {formatElapsed(elapsedSeconds)}</>}
    </p>
  );
}

/** Seconds up to a minute, then minutes and seconds. */
export function formatElapsed(totalSeconds: number): string {
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${seconds.toString().padStart(2, '0')}s`;
}
