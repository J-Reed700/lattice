import { useEffect, useState } from "react";

/** About how long a spoken interview answer runs before it loses the listener. */
const AIM_SECONDS = 120;

function clock(seconds: number) {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/**
 * How long the learner has been answering the interviewer's latest question,
 * against the two minutes or so a spoken answer should take. Practising at the
 * real pace makes the answer transfer to the real room.
 */
export function AnswerTimer({ since }: { since: string }) {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    const started = Date.now();
    setElapsed(0);
    const timer = window.setInterval(() => setElapsed(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [since]);
  const over = elapsed > AIM_SECONDS;
  return (
    <p className="text-xs text-text-muted" aria-live="off">
      Answering for <span className={`tabular-nums ${over ? "font-medium text-text-primary" : ""}`}>{clock(elapsed)}</span>
      {over ? " · past two minutes: land your result and stop" : " · aim for about two minutes, out loud"}
    </p>
  );
}
