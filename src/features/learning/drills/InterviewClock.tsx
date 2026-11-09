import { ElapsedClock, formatClock } from '@/features/learning/practice/shared/ElapsedClock';

export { formatClock };

/**
 * The interview's time left, counted from the moment the attempt started. At zero it says so and goes on: the
 * review is the learner's to hand in.
 *
 * Delegates to the shared ElapsedClock component in countdown mode.
 */
export function InterviewClock({ startedAt, limitMs }: { startedAt: number; limitMs: number }) {
  return <ElapsedClock startedAt={startedAt} budgetMs={limitMs} countdown={true} />;
}
