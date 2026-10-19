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
      <div role="status">
        <h3 className="flex items-center gap-2 text-sm font-semibold text-text-primary">
          <ClipboardCheck size={18} className="shrink-0 text-accent" aria-hidden="true" />
          Lesson preparation is waiting for you
        </h3>
      </div>
      <p className="mt-2 text-sm leading-6 text-text-secondary">Every check that could run has run; the draft and all completed checks are saved. What holds {lessonIds.length === 1 ? 'the lesson' : 'each lesson'} is shown below.</p>
      {lessonIds.map((lessonId) => (
        <LessonClaimReviewPanel key={lessonId} programId={programId} lessonId={lessonId} courseRevision={revision} />
      ))}
    </section>
  );
}

/**
 * "Pass 3 · 4 claims sent back for stronger sources · 2 being repaired · 5
 * waiting for you · 108 verified" (§6.3). The machine's claims are named by
 * what is done to them next; only a rewrite of their text is a repair.
 */
export function claimTallyLine(pass: number, tally: LearningClaimTally): string {
  const machine: [number, string][] = [
    [tally.unchecked, 'not checked yet'],
    [tally.sentBack, 'sent back for stronger sources'],
    [tally.searching, 'looking for sources'],
    [tally.repairing, 'being repaired'],
  ];
  return [
    `Pass ${pass}`,
    ...machine.filter(([count]) => count > 0).map(([count, what], index) => index === 0 ? `${count} ${count === 1 ? 'claim' : 'claims'} ${what}` : `${count} ${what}`),
    `${tally.waiting} waiting for you`,
    `${tally.verified} verified`,
    tally.resolvedByYou ? `${tally.resolvedByYou} resolved by you` : null,
  ].filter(Boolean).join(' · ');
}
