import type { CourseDrillsDto } from '@/lib/bindings';

/** A course's drills view in a language, read again when a build of the course finishes. */
export function useCourseDrills(
  _programId: string,
  _language: string | null,
): { view: CourseDrillsDto | null; error: string | null; reload: () => void } {
  return { view: null, error: null, reload: () => undefined };
}
