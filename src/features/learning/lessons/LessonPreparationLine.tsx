import { CirclePause, LoaderCircle } from 'lucide-react';

import { preparationState, preparationSummary } from '@/features/learning/lessons/lessonPreparationActivity';
import { useReloadWhenPrepared } from '@/features/learning/lessons/LessonPreparationProgress';
import type { LearningGenerationJob } from '@/lib/bindings';

/**
 * The lesson preparation job in one line, for tabs other than Lessons, where
 * the full progress panel would push the tab's own content off screen.
 */
export function LessonPreparationLine({ job, pending, programId, error, onView }: {
  job?: LearningGenerationJob; pending: boolean; programId: string; error?: string; onView: () => void;
}) {
  useReloadWhenPrepared(job, programId);
  const state = preparationState(job, pending);
  const summary = preparationSummary(job, state, error);
  if (!summary) return null;
  const working = state.active && !state.reconnecting;
  return <section role="region" aria-label="Lesson preparation" className="my-3 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border-default bg-surface px-4 py-2.5 shadow-2xs">
    <p role="status" className="flex min-w-0 items-center gap-2 text-xs text-text-primary">
      {working ? <LoaderCircle size={15} className="shrink-0 animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" /> : <CirclePause size={15} className="shrink-0 text-accent" aria-hidden="true" />}
      <span className="min-w-0 wrap-break-word font-medium">{summary}</span>
    </p>
    <button type="button" onClick={onView} className="rounded-lg border border-border-default bg-surface-sunken px-3 py-1.5 text-xs font-medium text-text-primary hover:bg-surface-raised transition-colors shadow-2xs">View</button>
  </section>;
}
