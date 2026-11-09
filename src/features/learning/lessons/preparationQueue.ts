import { currentPreparationJobs, heldLessonIds } from '@/features/learning/lessons/review/claimReviewPresentation';
import type { LearningGenerationJob, LearningLessonDto } from '@/lib/bindings';

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

/** The unfinished outline lessons that no pending or running job covers and no review holds. */
export function unqueuedLessons(lessons: LearningLessonDto[], jobs: LearningGenerationJob[]): LearningLessonDto[] {
  const queue = preparationQueue(jobs);
  const held = new Set(currentPreparationJobs(jobs).flatMap(heldLessonIds));
  return lessons.filter((lesson) => lesson.preparation === 'outline' && !lesson.completed && !queuedLesson(queue, lesson.id) && !held.has(lesson.id));
}

export type CoursePreparation = { ready: number; total: number; preparing: number; waiting: number; attention: number };

/** Where each lesson of the course stands: ready, running, queued, or failed or held for review. */
export function coursePreparation(lessons: LearningLessonDto[], jobs: LearningGenerationJob[]): CoursePreparation {
  const queue = preparationQueue(jobs);
  const current = currentPreparationJobs(jobs);
  const held = new Set(current.flatMap(heldLessonIds));
  const failed = new Set(current.filter((job) => job.status === 'failed' && !job.endedByUpdate).flatMap((job) => job.lessonIds ?? []));
  const counts = { ready: 0, total: lessons.length, preparing: 0, waiting: 0, attention: 0 };
  for (const lesson of lessons) {
    if (lesson.preparation === 'ready') { counts.ready += 1; continue; }
    if (lesson.completed) continue;
    const place = queuedLesson(queue, lesson.id);
    if (place) counts[place.job.status === 'running' ? 'preparing' : 'waiting'] += 1;
    else if (held.has(lesson.id) || failed.has(lesson.id)) counts.attention += 1;
  }
  return counts;
}

/** "4 of 18 lessons ready · 2 preparing · 11 waiting · 1 needs attention", the parts that are zero left out; null when nothing is in flight. */
export function coursePreparationLine(course: CoursePreparation): string | null {
  const { ready, total, preparing, waiting, attention } = course;
  if (preparing + waiting + attention === 0) return null;
  return [
    `${ready} of ${total} lessons ready`,
    preparing > 0 ? `${preparing} preparing` : null,
    waiting > 0 ? `${waiting} waiting` : null,
    attention > 0 ? `${attention} ${attention === 1 ? 'needs' : 'need'} attention` : null,
  ].filter(Boolean).join(' · ');
}

/** Whole minutes between lessons, the median gap between the finish times of this course's prepared lessons; null before two have finished. */
export function preparationPaceMinutes(jobs: LearningGenerationJob[]): number | null {
  const finishes = currentPreparationJobs(jobs)
    .filter((job) => job.status === 'completed' && !job.endedByUpdate && heldLessonIds(job).length === 0)
    .flatMap((job) => (job.finishedAt == null ? [] : [job.finishedAt]))
    .sort((a, b) => a - b);
  if (finishes.length < 2) return null;
  const gaps = finishes.slice(1).map((finish, index) => finish - (finishes[index] ?? 0)).sort((a, b) => a - b);
  const middle = Math.floor(gaps.length / 2);
  const median = gaps.length % 2 ? gaps[middle] ?? 0 : ((gaps[middle - 1] ?? 0) + (gaps[middle] ?? 0)) / 2;
  return Math.max(1, Math.round(median / 60_000));
}

export const paceLine = (minutes: number) => `About one lesson every ${minutes} min so far`;
