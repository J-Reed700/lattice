import { CirclePause, LoaderCircle } from 'lucide-react';

import { preparationState, preparationSummary } from '@/features/learning/lessons/lessonPreparationActivity';
import { useReloadWhenPrepared } from '@/features/learning/lessons/LessonPreparationProgress';
import type { LearningGenerationJob } from '@/lib/bindings';

/**
 * The lesson preparation job in one line, for tabs other than Lessons, where
 * the full progress panel would push the tab's own content off screen.
 */
export function LessonPreparationLine({ job, queuedMore = 0, failedCount = 0, pending, programId, error, onView, onManageQueue }: {
  job?: LearningGenerationJob; queuedMore?: number; failedCount?: number; pending: boolean; programId: string; error?: string; onView: () => void; onManageQueue?: () => void;
}) {
  useReloadWhenPrepared(job, programId);
  const state = preparationState(job, pending);
  const summary = preparationSummary(job, state, error, queuedMore);
  const failedNote = failedCount > 0 ? `${failedCount} failed` : null;
  const line = summary && failedNote ? `${summary} · ${failedNote}` : summary ?? (failedCount > 0 ? `Lesson preparation failed (${failedCount})` : null);
  if (!line) return null;
  const working = state.active && !state.reconnecting;
  const canManageQueue = Boolean(
    onManageQueue && (queuedMore > 0 || state.active || state.paused || failedCount > 0 || (job && job.status !== 'completed')),
  );
  return <section aria-label="Lesson preparation" className="my-3 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-accent/30 bg-accent/5 px-4 py-2.5">
    <p role="status" className="flex min-w-0 items-center gap-2 text-sm text-text-primary">
      {working ? <LoaderCircle size={16} className="shrink-0 animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" /> : <CirclePause size={16} className="shrink-0 text-accent" aria-hidden="true" />}
      <span className="min-w-0 wrap-break-word">{line}</span>
    </p>
    <div className="flex items-center gap-2">
      {canManageQueue && (
        <button type="button" onClick={onManageQueue} className="rounded-full border border-accent/40 bg-bg px-3 py-1.5 text-xs font-medium text-text-primary">
          Manage queue
        </button>
      )}
      <button type="button" onClick={onView} className="rounded-full border border-accent/40 bg-bg px-3 py-1.5 text-xs font-medium text-text-primary">View</button>
    </div>
  </section>;
}
