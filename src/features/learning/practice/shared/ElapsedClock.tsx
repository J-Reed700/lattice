import type React from 'react';
import { useEffect, useRef, useState } from 'react';
import { Clock, Timer } from 'lucide-react';

export interface ElapsedClockRenderParams {
  formatted: string;
  elapsedMs: number;
  isOvertime: boolean;
  running: boolean;
}

export interface ElapsedClockProps {
  elapsedMs?: number;
  baseMs?: number;
  startedAt?: number | string | null;
  since?: number | null;
  running?: boolean;
  budgetMs?: number | null;
  limitMs?: number;
  label?: string;
  className?: string;
  countdown?: boolean;
  prefix?: string;
  render?: (params: ElapsedClockRenderParams) => React.ReactNode;
}

/** "9:05" for 545 seconds; a part of a second counts as the whole of it. */
export function formatClock(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

export function ElapsedClock({
  elapsedMs,
  baseMs,
  startedAt,
  since,
  running = true,
  budgetMs,
  limitMs,
  label,
  className = '',
  countdown = false,
  prefix,
  render,
}: ElapsedClockProps): JSX.Element {
  const effectiveElapsedMs = elapsedMs ?? baseMs;
  const effectiveStartedAt = startedAt !== undefined ? startedAt : since;
  const effectiveRunning = since !== undefined ? since !== null : running;
  const effectiveBudgetMs = budgetMs !== undefined ? budgetMs : limitMs;

  const [currentMs, setCurrentMs] = useState<number>(() => {
    if (effectiveStartedAt !== undefined && effectiveStartedAt !== null && effectiveRunning) {
      const start = typeof effectiveStartedAt === 'string' ? new Date(effectiveStartedAt).getTime() : effectiveStartedAt;
      return (effectiveElapsedMs ?? 0) + Math.max(0, Date.now() - start);
    }
    if (effectiveElapsedMs !== undefined) return effectiveElapsedMs;
    return 0;
  });

  const baseElapsedRef = useRef(effectiveElapsedMs ?? 0);
  const startRef = useRef(effectiveStartedAt);

  useEffect(() => {
    baseElapsedRef.current = effectiveElapsedMs ?? 0;
    startRef.current = effectiveStartedAt;
  }, [effectiveElapsedMs, effectiveStartedAt]);

  useEffect(() => {
    if (effectiveElapsedMs !== undefined && (effectiveStartedAt === undefined || effectiveStartedAt === null)) {
      setCurrentMs(effectiveElapsedMs);
      return;
    }
    if (!effectiveRunning) {
      if (effectiveElapsedMs !== undefined) {
        setCurrentMs(effectiveElapsedMs);
      }
      return;
    }

    const baseOffset = effectiveStartedAt !== undefined && effectiveStartedAt !== null ? (effectiveElapsedMs ?? 0) : 0;
    const startTimestamp =
      effectiveStartedAt !== undefined && effectiveStartedAt !== null
        ? typeof effectiveStartedAt === 'string'
          ? new Date(effectiveStartedAt).getTime()
          : effectiveStartedAt
        : Date.now() - (effectiveElapsedMs ?? 0);

    const update = () => {
      const elapsed = baseOffset + Math.max(0, Date.now() - startTimestamp);
      setCurrentMs(elapsed);
    };

    update();
    const interval = window.setInterval(update, 1000);
    return () => window.clearInterval(interval);
  }, [effectiveStartedAt, effectiveRunning, effectiveElapsedMs]);

  const isOvertime = effectiveBudgetMs !== undefined && effectiveBudgetMs !== null && currentMs > effectiveBudgetMs;

  if (render) {
    return <>{render({ formatted: formatClock(currentMs), elapsedMs: currentMs, isOvertime, running: effectiveRunning })}</>;
  }

  // Countdown mode (used by InterviewClock)
  if (countdown && effectiveBudgetMs !== undefined && effectiveBudgetMs !== null) {
    const left = effectiveBudgetMs - currentMs;
    if (left <= 0) {
      return (
        <p
          role="status"
          aria-label={label}
          className={`inline-flex items-center gap-1.5 rounded-full bg-warning-muted px-3 py-1 text-xs font-semibold text-warning-fg ${className}`}
        >
          <Timer size={13} aria-hidden="true" />
          <span>Time is up. You can keep going or hand in your review now.</span>
        </p>
      );
    }
    return (
      <p
        role="timer"
        aria-live="off"
        aria-label={label}
        className={`inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-3 py-1 text-xs text-text-secondary ${className}`}
      >
        <Timer size={13} aria-hidden="true" />
        <span>
          Time left{' '}
          <span className="font-semibold tabular-nums text-text-primary">
            {formatClock(left)}
          </span>
        </span>
      </p>
    );
  }

  return (
    <div className={`inline-flex flex-wrap items-center gap-2 text-xs ${className}`}>
      <p
        role="timer"
        aria-live="off"
        aria-label={label}
        className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-3 py-1 text-text-secondary"
      >
        <Clock size={13} aria-hidden="true" />
        <span>
          {prefix && <span className="text-text-muted">{prefix} </span>}
          <span className="font-semibold tabular-nums text-text-primary">
            {formatClock(currentMs)}
          </span>
          {effectiveBudgetMs !== undefined && effectiveBudgetMs !== null && (
            <span className="text-text-muted"> / {formatClock(effectiveBudgetMs)}</span>
          )}
        </span>
      </p>

      {isOvertime && (
        <span
          role="status"
          aria-live="polite"
          className="inline-flex items-center gap-1 rounded-full bg-warning-muted px-2.5 py-0.5 font-medium text-warning-fg"
        >
          Time is up · Overtime ({formatClock(currentMs - (effectiveBudgetMs ?? 0))} past budget)
        </span>
      )}
    </div>
  );
}
