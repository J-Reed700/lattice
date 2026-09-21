import { useEffect, useState } from 'react';

import type { TurnStep } from '@/types/conversation';

/** One step of the timeline: what, what came of it, how long. */
export function StepRow({ step }: { step: TurnStep }) {
  const elapsed = useElapsedSeconds(step.state === 'running' ? step.id : null);
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
    </li>
  );
}

/** Track seconds since a step started running, restarting on each new step. */
export function useElapsedSeconds(runningStepId: string | null): number {
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    if (!runningStepId) {
      setSeconds(0);
      return;
    }
    const startedAt = Date.now();
    setSeconds(0);
    const timer = setInterval(() => {
      setSeconds(Math.floor((Date.now() - startedAt) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [runningStepId]);
  return seconds;
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
