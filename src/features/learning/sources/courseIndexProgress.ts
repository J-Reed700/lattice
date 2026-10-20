/**
 * How a course index's progress is put into words: the course workspace's
 * notice and each lesson action that waits for it say it the same way. The
 * counts and the rounding of time left follow Explorer's folder index.
 */
import type { CourseIndexStatusDto } from '@/lib/bindings';

export type IndexingStatus = Extract<CourseIndexStatusDto, { state: 'indexing' }>;

/** `13,457`. */
export const count = (value: number) => value.toLocaleString('en-US');

/** Whole percent of passages embedded; never 100 before the last one is in. */
export function indexPercent(embedded: number, total: number): number {
  if (total <= 0) return 0;
  return Math.floor((Math.min(embedded, total) / total) * 100);
}

/** "4,210 of 13,457 passages", or the new references being added before any are counted. */
export function describeIndexCounts(status: IndexingStatus): string | null {
  if (status.total > 0) return `${count(status.embedded)} of ${count(status.total)} passages`;
  if (status.referencesNew > 0) return `Adding ${count(status.referencesNew)} new ${status.referencesNew === 1 ? 'reference' : 'references'}`;
  return null;
}

/** Seconds until every passage is embedded at the build's measured pace; `null` until it has one. */
export function secondsLeft(status: IndexingStatus): number | null {
  const pace = status.passagesPerSecond;
  if (!pace || pace <= 0 || status.total <= 0) return null;
  return Math.max(0, status.total - status.embedded) / pace;
}

/** Time left as people say it: "less than a minute", "about 11 min", "about 1 h 10 min". */
export function formatTimeLeft(seconds: number): string {
  if (seconds < 60) return 'less than a minute';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `about ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `about ${hours} h ${rest} min` : `about ${hours} h`;
}

/**
 * "about 11 min left" once the build has a pace; "measuring speed…" while it
 * runs without one; `null` while it waits its turn, which has no estimate.
 */
export function describeTimeLeft(status: IndexingStatus): string | null {
  if (status.waitingTurn) return null;
  const seconds = secondsLeft(status);
  return seconds === null ? 'measuring speed…' : `${formatTimeLeft(seconds)} left`;
}

/** What a held lesson action says before how long is left. */
export const WAITS_FOR_INDEX = 'Waits for the course index';

/**
 * Why a lesson action waits, shown beside it: "Waits for the course index ·
 * about 11 min left". `undefined` when the course is not indexing.
 */
export function describeIndexWait(status: CourseIndexStatusDto | null): string | undefined {
  if (status?.state !== 'indexing') return undefined;
  if (status.waitingTurn) return `${WAITS_FOR_INDEX} · queued behind another course`;
  const left = describeTimeLeft(status);
  return status.total > 0 && left ? `${WAITS_FOR_INDEX} · ${left}` : WAITS_FOR_INDEX;
}
