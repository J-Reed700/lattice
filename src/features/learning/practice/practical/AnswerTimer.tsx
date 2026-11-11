import { useEffect, useRef, useState } from "react";

/** About how long a spoken interview answer runs before it loses the listener. */
const AIM_SECONDS = 120;

function clock(seconds: number) {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/**
 * How long the learner has been answering the interviewer's latest question,
 * against the two minutes or so a spoken answer should take. Practising at the
 * real pace makes the answer transfer to the real room.
 *
 * The clock stops while `running` is false: once the answer is sent, the wait
 * for the interviewer's reply (a minute or more on a local model) is not
 * answering time. A failed send resumes it from where it stopped.
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
      setElapsed(counted.current.seconds);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [since, running]);
  const over = elapsed > AIM_SECONDS;
  const time = <span className={`tabular-nums ${over ? "font-medium text-text-primary" : ""}`}>{clock(elapsed)}</span>;
  return running ? (
    <p className="text-xs text-text-muted" aria-live="off">
      Answering for {time}
      {over ? " · past two minutes: land your result and stop" : " · aim for about two minutes, out loud"}
    </p>
  ) : (
    <p className="text-xs text-text-muted" aria-live="off">
      Answered in {time}
      {over && " · over two minutes"}
    </p>
  );
}
