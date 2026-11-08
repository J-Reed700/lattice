import type { DrillOptions } from './drillOptions';

/** The job that writes a drill. */
export const DRILL_BUILD = 'learning.drill_build';
/** The job that grades a review. */
export const DRILL_GRADING = 'learning.drill_grading';

/** The subject of the jobs that build for the shelf a drill is taken from: the course's passages, or its code in a language. */
export function shelfOf(programId: string, options: DrillOptions): string {
  return options.material === 'passage' ? `drill:${programId}:passage` : `drill:${programId}:code:${options.language}`;
}
