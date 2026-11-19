import type { CoursePlanEntryDto, CoursePlanStateDto, CoursePlanViewDto, PlannedLesson } from '@/lib/bindings';

export function topics(lessonId: string, overrides: Partial<PlannedLesson> = {}): PlannedLesson {
  return {
    lessonId,
    ideas: [`${lessonId} first idea`, `${lessonId} second idea`],
    terms: [`${lessonId} term`],
    actions: [`${lessonId} action`],
    contrast: `${lessonId} contrast`,
    ...overrides,
  };
}

export function entry(lessonId: string, overrides: Partial<CoursePlanEntryDto> = {}): CoursePlanEntryDto {
  return { topics: topics(lessonId), entryRevision: 1, editable: true, previous: null, ...overrides };
}

export function planView(entries: CoursePlanEntryDto[], overrides: Partial<CoursePlanViewDto> = {}): CoursePlanViewDto {
  return { programId: 'program-1', outlineRevision: 4, state: 'planned', error: null, entries, ...overrides };
}

export function emptyPlan(state: CoursePlanStateDto, error: string | null = null): CoursePlanViewDto {
  return planView([], { state, error });
}

export const ok = <T,>(data: T) => ({ ok: true as const, data });
