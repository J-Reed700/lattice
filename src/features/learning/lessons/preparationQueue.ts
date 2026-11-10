import { currentPreparationJobs } from '@/features/learning/lessons/review/claimReviewPresentation';
import type { LearningGenerationJob } from '@/lib/bindings';

/** The course's lesson jobs that are pending or running, in the order they were queued. */
export function preparationQueue(jobs: LearningGenerationJob[]): LearningGenerationJob[] {
  return currentPreparationJobs(jobs)
    .filter((job) => !job.endedByUpdate && (job.status === 'pending' || job.status === 'running'))
    .sort((a, b) => a.createdAt - b.createdAt);
}

/** The lesson's place in the queue: its job and how many jobs were queued before it. */
export function queuedLesson(queue: LearningGenerationJob[], lessonId: string): { job: LearningGenerationJob; ahead: number } | null {
  const job = queue.find((candidate) => candidate.lessonIds?.includes(lessonId));
  return job ? { job, ahead: queue.indexOf(job) } : null;
}

/** "Queued · 2 ahead", or "Queued" when nothing is ahead. */
export function queuedLine(ahead: number): string {
  return ahead > 0 ? `Queued · ${ahead} ahead` : 'Queued';
}
