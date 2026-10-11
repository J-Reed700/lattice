import { createContext, useContext, useEffect, useState } from 'react';

import type { TurnStep } from '@/types/conversation';

/**
 * When the in-flight turn began, as a wall clock.
 *
 * A step's `startedAtMs` is an offset from the start of its turn, so a row
 * needs that origin to say how old a running step is. `null` for a turn that
 * is over, where every step carries the duration it took.
 */
export const TurnClockContext = createContext<number | null>(null);

/** One step of the timeline: what, what came of it, how long. */
export function StepRow({ step }: { step: TurnStep }) {
  const elapsed = useElapsedSeconds(step.state === 'running' ? step : null);
  return (
    <li className="turn-record-step" data-state={step.state} data-kind={step.kind}>
      <span className="turn-record-dot" aria-hidden="true" />
      <span className="turn-record-label">
        {step.label}
        {step.detail && <span className="turn-record-detail"> · {step.detail}</span>}
        {step.result && <span className="turn-record-result"> · {step.result}</span>}
      </span>
      <span className="turn-record-time">
        {step.state === 'running'
          ? // A clock, not a spinner: a label alone cannot tell "started a
            // second ago" from "stuck here for six minutes".
            elapsed >= 1
            ? formatDuration(elapsed * 1000)
            : ''
          : typeof step.durationMs === 'number'
            ? formatDuration(step.durationMs)
            : ''}
      </span>
      <ReasoningDisclosure reasoning={step.reasoning} />
    </li>
  );
}

export function ReasoningDisclosure({ reasoning }: { reasoning?: string | null }) {
  const text = reasoning?.trim();
  if (!text) return null;
  return (
    <details className="turn-record-reasoning">
      <summary>Reasoning</summary>
      <div className="turn-record-reasoning-text">{text}</div>
    </details>
  );
}

/**
 * Seconds a step has been running, ticking once a second.
 *
 * Timed from when the step started, not from when this row mounted, so the
 * clock reads the step's age however many times the row comes and goes:
 * folding the record and opening it again used to send every clock back to
 * zero while the step itself ran on. Without the turn's origin — a record
 * rendered outside a live turn — timing from here is the best it can do.
 */
export function useElapsedSeconds(step: TurnStep | null): number {
  const turnStartedAt = useContext(TurnClockContext);
  const stepId = step?.id ?? null;
  const startedAt = step && turnStartedAt !== null ? turnStartedAt + step.startedAtMs : null;
  // Right on the first paint, so a reopened record shows the age it left off
  // at rather than a zero that corrects itself a second later.
  const [seconds, setSeconds] = useState(() => (startedAt === null ? 0 : secondsSince(startedAt)));
  useEffect(() => {
    if (!stepId) {
      setSeconds(0);
      return;
    }
    const from = startedAt ?? Date.now();
    setSeconds(secondsSince(from));
    const timer = setInterval(() => setSeconds(secondsSince(from)), 1000);
    return () => clearInterval(timer);
  }, [startedAt, stepId]);
  return seconds;
}

/** Whole seconds since a wall clock, never negative however the clocks differ. */
function secondsSince(from: number): number {
  return Math.max(0, Math.floor((Date.now() - from) / 1000));
}

/**
 * Milliseconds as a duration a reader can compare at a glance.
 *
 * Shared because the same turn appears in more than one place and the two must
 * not disagree about how long it took.
 */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const whole = Math.round(seconds);
  return `${Math.floor(whole / 60)}m ${String(whole % 60).padStart(2, '0')}s`;
}
