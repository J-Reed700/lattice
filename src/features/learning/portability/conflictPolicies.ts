import type { LearningPackConflictDto, LearningPackConflictKind, LearningPackConflictPolicy, LearningPackImportPreviewDto } from '@/lib/bindings';

export type ConflictPolicyOption = { value: LearningPackConflictPolicy; label: string; description: string; suggested?: true };

/** A file is first previewed as a move; the other choices are offered only when that preview is blocked. */
export const DEFAULT_CONFLICT_POLICY: LearningPackConflictPolicy = 'merge_safe';

const POLICIES: Record<LearningPackConflictPolicy, Omit<ConflictPolicyOption, 'value'>> = {
  merge_safe: { label: 'Move it here', description: 'Keeps the course and your progress exactly as exported. For a course that is not on this computer yet.' },
  // Highlighted wherever it is offered: it keeps the course's identity and history.
  replace_after_backup: { label: 'Replace the copy on this computer', description: "A backup of this computer's copy is saved first; then the pack's version takes its place with the same course identity.", suggested: true },
  create_copy: { label: 'Add as a separate copy', description: 'Keeps both. The imported course gets new identifiers, so its history is separate.' },
};

export const conflictPolicyOption = (value: LearningPackConflictPolicy): ConflictPolicyOption => ({ value, ...POLICIES[value] });

/** The one source of the import choices' wording, for the panel, the import flow and tests. */
export const CONFLICT_POLICIES: readonly ConflictPolicyOption[] = (['merge_safe', 'replace_after_backup', 'create_copy'] as const).map(conflictPolicyOption);

const owned = (label: string) => ({
  label,
  reason: (title: string | null) => title
    ? `${label} “${title}” already belongs to another course on this computer.`
    : `A ${label.toLowerCase()} in this file already belongs to another course on this computer.`,
});
const fixed = (label: string, reason: string, more: { retryable?: true; blocksExport?: string } = {}) => ({ label, reason: () => reason, ...more });

/**
 * Wording for each conflict kind. Which choices clear a conflict comes from the
 * backend (`clearsWith`); `retryable` marks a block that can clear by itself,
 * so previewing (or exporting) again later is worth offering. `blocksExport`
 * words the kind when it holds back an export rather than an import.
 */
export const CONFLICT_KINDS: Record<LearningPackConflictKind, { label: string; reason(title: string | null): string; retryable?: true; blocksExport?: string }> = {
  pack_schema: fixed('Different build', 'This file was made by a different build of Lattice. Run the same version on both computers, export the course again, then import the new file.'),
  program: fixed('Course', 'This course is already on this computer.'),
  program_history: fixed('Course history', 'The copy on this computer has history this file cannot restore, so it cannot be replaced.'),
  program_jobs: fixed('Course preparation', 'The copy on this computer is still being prepared. Wait for it to finish before replacing it.', {
    retryable: true,
    blocksExport: 'This course is still being prepared. You can export it when preparation finishes.',
  }),
  module: owned('Module'),
  lesson: owned('Lesson'),
  question: owned('Question'),
  attempt: owned('Attempt'),
  record: owned('Record'),
};

/** The parts of a preview that decide what can happen next. */
type PreviewConflicts = Pick<LearningPackImportPreviewDto, 'conflictPolicy' | 'conflicts'>;

/** A conflict stands under the preview's choice exactly when that choice is not one that clears it. */
export const isBlocked = (conflict: LearningPackConflictDto, policy: LearningPackConflictPolicy) => !conflict.clearsWith.includes(policy);

export const blockedConflicts = (preview: PreviewConflicts) =>
  preview.conflicts.filter((conflict) => isBlocked(conflict, preview.conflictPolicy));

/** The "already on this computer" case: a move-in preview of a course that exists here. */
export const existingCourseConflict = (preview: PreviewConflicts) =>
  preview.conflictPolicy === DEFAULT_CONFLICT_POLICY ? blockedConflicts(preview).find((conflict) => conflict.kind === 'program') : undefined;

/** The other choices that clear every blocked conflict of this preview, in the catalogue's order. */
export function proceedingPolicies(preview: PreviewConflicts): LearningPackConflictPolicy[] {
  const blocked = blockedConflicts(preview);
  if (blocked.length === 0) return [];
  return CONFLICT_POLICIES.map(({ value }) => value)
    .filter((value) => value !== preview.conflictPolicy && blocked.every((conflict) => conflict.clearsWith.includes(value)));
}

/** Blocked by something that can clear by itself, and by nothing no choice can ever clear: previewing again later may pass. */
export function awaitsPreparation(preview: PreviewConflicts) {
  const blocked = blockedConflicts(preview);
  return blocked.some((conflict) => CONFLICT_KINDS[conflict.kind].retryable)
    && blocked.every((conflict) => CONFLICT_KINDS[conflict.kind].retryable || conflict.clearsWith.length > 0);
}

/** Plain-language reason for a blocked conflict: the backend's own, else the kind's wording. */
export function blockedReason(conflict: LearningPackConflictDto): string {
  return conflict.reason?.trim() || CONFLICT_KINDS[conflict.kind].reason(conflict.existingTitle?.trim() || null);
}
