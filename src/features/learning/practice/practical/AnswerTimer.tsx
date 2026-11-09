import { useEffect, useRef, useState } from 'react';
import { ElapsedClock } from '@/features/learning/practice/shared/ElapsedClock';

/** About how long a spoken interview answer runs before it loses the listener. */
const AIM_MS = 120_000;

/**
 * How long the learner has been answering the interviewer's latest question,
 * against the two minutes or so a spoken answer should take. Practising at the
 * real pace makes the answer transfer to the real room.
 *
 * Delegates to the shared ElapsedClock component.
 */
export function AnswerTimer({ since, running = true }: { since: string; running?: boolean }) {
  const [elapsed, setElapsed] = useState(0);
  const counted = useRef({ since, seconds: 0 });

  useEffect(() => {
    if (counted.current.since !== since) {
      counted.current = { since, seconds: 0 };
      setElapsed(0);
    }
    if (!running) return;
    const resumed = Date.now() - counted.current.seconds * 1000;
    const timer = window.setInterval(() => {
      counted.current.seconds = Math.floor((Date.now() - resumed) / 1000);
      setElapsed(counted.current.seconds * 1000);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [since, running]);

  return (
    <ElapsedClock
      elapsedMs={elapsed}
      running={running}
      budgetMs={AIM_MS}
      render={({ formatted, isOvertime }) => {
        const time = (
          <span className={`tabular-nums ${isOvertime ? 'font-medium text-text-primary' : ''}`}>
            {formatted}
          </span>
        );
        return running ? (
          <p className="text-xs text-text-muted" aria-live="off">
            Answering for {time}
            {isOvertime
              ? ' · past two minutes: land your result and stop'
              : ' · aim for about two minutes, out loud'}
          </p>
        ) : (
          <p className="text-xs text-text-muted" aria-live="off">
            Answered in {time}
            {isOvertime && ' · over two minutes'}
          </p>
        );
      }}
    />
  );
}
