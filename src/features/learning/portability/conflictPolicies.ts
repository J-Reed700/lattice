import type { LearningPackConflictDto, LearningPackConflictPolicy, LearningPackImportPreviewDto } from '@/lib/bindings';

export type ConflictPolicyOption = { value: LearningPackConflictPolicy; label: string; description: string };

/** The one source of the import choices' wording, for the panel, the import flow and tests. */
export const CONFLICT_POLICIES: readonly ConflictPolicyOption[] = [
  { value: 'merge_safe', label: 'Move it here', description: 'Keeps the course and your progress exactly as exported. For a course that is not on this computer yet.' },
  { value: 'replace_after_backup', label: 'Replace the copy on this computer', description: "A backup of this computer's copy is saved first; then the pack's version takes its place with the same course identity." },
  { value: 'create_copy', label: 'Add as a separate copy', description: 'Keeps both. The imported course gets new identifiers, so its history is separate.' },
];

export function conflictPolicyOption(value: LearningPackConflictPolicy): ConflictPolicyOption {
  return CONFLICT_POLICIES.find((option) => option.value === value) ?? CONFLICT_POLICIES[0];
}

/** The parts of a preview that decide what can happen next. */
type PreviewConflicts = Pick<LearningPackImportPreviewDto, 'conflictPolicy' | 'conflicts'>;

export const blockedConflicts = (preview: PreviewConflicts) =>
  preview.conflicts.filter((conflict) => conflict.resolution === 'blocked');

/** The "already on this computer" case: a move-in (merge_safe) preview of a course that exists here. */
export const existingCourseConflict = (preview: PreviewConflicts) =>
  preview.conflictPolicy === 'merge_safe' ? blockedConflicts(preview).find((conflict) => conflict.entityKind === 'program') : undefined;

// No choice gets past these: a different build of Lattice, or a course here
// that is still being prepared (that one can clear, see `awaitsPreparation`).
const NO_CHOICE = new Set(['pack_schema', 'program_jobs']);

/**
 * The other choices that can get past this preview's blocked conflicts.
 * A course that is already here can be replaced, unless another course owns
 * some of its records. A copy gets new identifiers, so it proceeds past every
 * other block.
 */
export function proceedingPolicies(preview: PreviewConflicts): LearningPackConflictPolicy[] {
  const blocked = blockedConflicts(preview);
  if (blocked.length === 0 || blocked.some((conflict) => NO_CHOICE.has(conflict.entityKind))) return [];
  const choices: LearningPackConflictPolicy[] = [];
  if (preview.conflictPolicy === 'merge_safe' && blocked.every((conflict) => conflict.entityKind === 'program')) choices.push('replace_after_backup');
  if (preview.conflictPolicy !== 'create_copy') choices.push('create_copy');
  return choices;
}

/** Blocked only until the course here finishes preparing, so previewing again later can clear it. */
export function awaitsPreparation(preview: PreviewConflicts) {
  const kinds = blockedConflicts(preview).map((conflict) => conflict.entityKind);
  return kinds.includes('program_jobs') && !kinds.includes('pack_schema');
}

// For these kinds the backend puts its own explanation in `existingTitle`.
const fallbackReasons: Record<string, string> = {
  pack_schema: 'This file was made by a different build of Lattice. Run the same version on both computers, export the course again, then import the new file.',
  program_jobs: 'The copy on this computer is still being prepared. Wait for it to finish before replacing it.',
  program_history: 'The copy on this computer has history this file cannot restore, so it cannot be replaced.',
};

/**
 * Plain-language reason for a blocked conflict. The course itself being here
 * (`program`) is worded by the import flow; the conflict list keeps the raw fields.
 */
export function blockedReason(conflict: LearningPackConflictDto): string {
  const fallback = fallbackReasons[conflict.entityKind];
  if (fallback) return conflict.existingTitle.trim() || fallback;
  // A record another course owns. `incomingTitle` is the record's own title,
  // or just its table name; `existingTitle` is never a course name here.
  const kind = conflict.entityKind.replace(/_/g, ' ');
  const title = conflict.incomingTitle.trim();
  const named = title && title !== conflict.entityKind ? ` “${title}”` : '';
  return `${kind.charAt(0).toUpperCase()}${kind.slice(1)}${named} already belongs to another course on this computer.`;
}
