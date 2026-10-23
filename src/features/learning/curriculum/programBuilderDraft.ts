import { clearDraft } from '@/shared/drafts/drafts';

/** What the course builder keeps while you are away from it, until a course is made from it. */
export const programBuilderDraftKeys = {
  goal: 'course-builder:goal',
  prior: 'course-builder:prior',
  urls: 'course-builder:urls',
} as const;

export function clearProgramBuilderDraft(): void {
  for (const key of Object.values(programBuilderDraftKeys)) clearDraft(key);
}
