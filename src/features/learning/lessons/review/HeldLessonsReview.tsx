import { ClipboardCheck } from 'lucide-react';

import { LessonClaimReviewPanel } from '@/features/learning/lessons/review/LessonClaimReviewPanel';
import type { LearningClaimTally } from '@/lib/bindings';

/**
 * A preparation job that ended with lessons held for review (§6.1): not a
 * failure. The claim lists are the main content, one per held lesson.
 */
export function HeldLessonsReview({ lessonIds, programId, revision }: {
  lessonIds: string[]; programId: string; revision: number;
}) {
  return (
    <section aria-label="Lesson preparation progress" className="my-4 rounded-2xl border border-accent/30 bg-accent/5 p-5">
      <h3 role="status" className="flex items-center gap-2 text-sm font-semibold text-text-primary">
        <ClipboardCheck size={18} className="shrink-0 text-accent" aria-hidden="true" />
        Lesson preparation is waiting for you
      </h3>
      <p className="mt-2 text-sm leading-6 text-text-secondary">Every check that could run has run; the draft and all completed checks are saved. What holds {lessonIds.length === 1 ? 'the lesson' : 'each lesson'} is shown below.</p>
      {lessonIds.map((lessonId) => (
        <LessonClaimReviewPanel key={lessonId} programId={programId} lessonId={lessonId} courseRevision={revision} />
      ))}
    </section>
  );
}

/** "Pass 3 · 2 claims being repaired · 5 waiting for you · 108 verified" (§6.3). */
export function claimTallyLine(pass: number, tally: LearningClaimTally): string {
  return [
    `Pass ${pass}`,
    `${tally.repairing} ${tally.repairing === 1 ? 'claim' : 'claims'} being repaired`,
    `${tally.waiting} waiting for you`,
    `${tally.verified} verified`,
    tally.resolvedByYou ? `${tally.resolvedByYou} resolved by you` : null,
  ].filter(Boolean).join(' · ');
}
