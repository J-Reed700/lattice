import { describe, expect, it } from 'vitest';

import { awaitsPreparation, blockedReason, CONFLICT_POLICIES, DEFAULT_CONFLICT_POLICY, proceedingPolicies } from '@/features/learning/portability/conflictPolicies';
import type { LearningPackConflictDto, LearningPackConflictKind, LearningPackConflictPolicy } from '@/lib/bindings';

// The choices the backend says clear each kind.
const CLEARS_WITH: Record<LearningPackConflictKind, LearningPackConflictPolicy[]> = {
  pack_schema: [], program: ['replace_after_backup', 'create_copy'], program_history: ['create_copy'], program_jobs: ['create_copy'],
  module: ['create_copy'], lesson: ['create_copy'], question: ['create_copy'], attempt: ['create_copy'], record: ['create_copy'],
};
const conflict = (kind: LearningPackConflictKind, overrides: Partial<LearningPackConflictDto> = {}): LearningPackConflictDto => ({ kind, existingId: 'course-1', existingTitle: 'Field biology', reason: null, clearsWith: CLEARS_WITH[kind], ...overrides });
const preview = (conflictPolicy: LearningPackConflictPolicy, conflicts: LearningPackConflictDto[]) => ({ conflictPolicy, conflicts });

describe('Import choices', () => {
  it('words the three choices as the design brief does, and previews a file as a move first', () => {
    expect(CONFLICT_POLICIES.map(({ value, label }) => [value, label])).toEqual([
      ['merge_safe', 'Move it here'],
      ['replace_after_backup', 'Replace the copy on this computer'],
      ['create_copy', 'Add as a separate copy'],
    ]);
    expect(DEFAULT_CONFLICT_POLICY).toBe('merge_safe');
    expect(CONFLICT_POLICIES.filter((option) => option.suggested).map(({ value }) => value)).toEqual(['replace_after_backup']);
  });

  it('offers the choices that clear every blocked conflict, as the backend reports them', () => {
    expect(proceedingPolicies(preview('merge_safe', [conflict('program')]))).toEqual(['replace_after_backup', 'create_copy']);
    expect(proceedingPolicies(preview('merge_safe', [conflict('program'), conflict('lesson', { existingId: 'lesson-1' })]))).toEqual(['create_copy']);
    expect(proceedingPolicies(preview('replace_after_backup', [conflict('program_history')]))).toEqual(['create_copy']);
    expect(proceedingPolicies(preview('create_copy', [conflict('program')]))).toEqual([]);
    expect(proceedingPolicies(preview('merge_safe', [conflict('program', { clearsWith: ['merge_safe', 'create_copy'] })]))).toEqual([]);
  });

  it('offers no choice for a different build, and a fresh check only for a course still being prepared', () => {
    const schema = preview('merge_safe', [conflict('pack_schema')]);
    const preparing = preview('replace_after_backup', [conflict('program_jobs'), conflict('program_history')]);
    expect(proceedingPolicies(schema)).toEqual([]);
    expect(proceedingPolicies(preparing)).toEqual(['create_copy']);
    expect(awaitsPreparation(schema)).toBe(false);
    expect(awaitsPreparation(preparing)).toBe(true);
    expect(awaitsPreparation(preview('replace_after_backup', [conflict('program_jobs'), conflict('pack_schema')]))).toBe(false);
    expect(awaitsPreparation(preview('create_copy', [conflict('program_jobs')]))).toBe(false);
  });

  it('names a record another course owns by its own title, or by its kind alone', () => {
    expect(blockedReason(conflict('lesson', { existingTitle: 'Reading a transect' }))).toBe('Lesson “Reading a transect” already belongs to another course on this computer.');
    expect(blockedReason(conflict('record', { existingTitle: null }))).toBe('A record in this file already belongs to another course on this computer.');
    expect(blockedReason(conflict('question', { existingTitle: ' ' }))).toBe('A question in this file already belongs to another course on this computer.');
  });

  it("uses the backend's own reason when it gives one, with the kind's wording as the fallback", () => {
    expect(blockedReason(conflict('program_jobs', { reason: 'This course is still being prepared.' }))).toBe('This course is still being prepared.');
    expect(blockedReason(conflict('program_jobs', { reason: ' ' }))).toBe('The copy on this computer is still being prepared. Wait for it to finish before replacing it.');
    expect(blockedReason(conflict('pack_schema', { existingTitle: null }))).toBe('This file was made by a different build of Lattice. Run the same version on both computers, export the course again, then import the new file.');
  });
});
