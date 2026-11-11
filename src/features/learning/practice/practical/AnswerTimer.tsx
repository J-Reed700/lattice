import { ElapsedClock } from '@/features/learning/practice/shared/ElapsedClock';

/** About how long a spoken interview answer runs before it loses the listener. */
const AIM_MS = 120_000;

/**
 * How long the learner has been answering the interviewer's latest question,
 * against the two minutes or so a spoken answer should take. A new question
 * (`since`) restarts it; while the answer is with the interviewer it holds.
 */
export function AnswerTimer({ since, running = true }: { since: string; running?: boolean }) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <ElapsedClock key={since} elapsedMs={0} running={running} softLimitMs={AIM_MS} label="Answer time" pastLimit="Past two minutes: land your result and stop." />
      <p className="text-xs text-text-secondary">
        {running ? 'Answering now. Aim for about two minutes, out loud.' : 'Answer sent.'}
      </p>
    </div>
  );
}
