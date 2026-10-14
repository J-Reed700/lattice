import { describe, expect, it } from 'vitest';

import { awaitsPreparation, blockedReason, CONFLICT_POLICIES, proceedingPolicies } from '@/features/learning/portability/conflictPolicies';
import type { LearningPackConflictDto, LearningPackConflictPolicy } from '@/lib/bindings';

const conflict = (entityKind: string, overrides: Partial<LearningPackConflictDto> = {}): LearningPackConflictDto => ({ entityKind, incomingId: 'course-1', incomingTitle: 'Field biology', existingId: 'course-1', existingTitle: 'Field biology', resolution: 'blocked', ...overrides });
const preview = (conflictPolicy: LearningPackConflictPolicy, conflicts: LearningPackConflictDto[]) => ({ conflictPolicy, conflicts });

describe('Import choices', () => {
  it('words the three choices as the design brief does', () => {
    expect(CONFLICT_POLICIES.map(({ value, label }) => [value, label])).toEqual([
      ['merge_safe', 'Move it here'],
      ['replace_after_backup', 'Replace the copy on this computer'],
      ['create_copy', 'Add as a separate copy'],
    ]);
  });

  it('offers replace or a copy for a course that is already here, and only a copy when another course owns its records', () => {
    expect(proceedingPolicies(preview('merge_safe', [conflict('program')]))).toEqual(['replace_after_backup', 'create_copy']);
    expect(proceedingPolicies(preview('merge_safe', [conflict('program'), conflict('lesson', { incomingId: 'lesson-1' })]))).toEqual(['create_copy']);
    expect(proceedingPolicies(preview('replace_after_backup', [conflict('program_history')]))).toEqual(['create_copy']);
    expect(proceedingPolicies(preview('merge_safe', [conflict('program', { resolution: 'create_copy_with_new_ids' })]))).toEqual([]);
  });

  it('offers no choice for a different build or a course still being prepared, and only the second can clear', () => {
    const schema = preview('merge_safe', [conflict('pack_schema')]);
    const preparing = preview('replace_after_backup', [conflict('program_jobs'), conflict('program_history')]);
    expect(proceedingPolicies(schema)).toEqual([]);
    expect(proceedingPolicies(preparing)).toEqual([]);
    expect(awaitsPreparation(schema)).toBe(false);
    expect(awaitsPreparation(preparing)).toBe(true);
    expect(awaitsPreparation(preview('replace_after_backup', [conflict('program_jobs'), conflict('pack_schema')]))).toBe(false);
  });

  it('names a record another course owns without passing its local title off as a course name', () => {
    expect(blockedReason(conflict('lesson', { incomingTitle: 'Reading a transect', existingTitle: 'Reading a transect (local)' }))).toBe('Lesson “Reading a transect” already belongs to another course on this computer.');
    expect(blockedReason(conflict('learning_source_versions', { incomingTitle: 'Field guide v2', existingTitle: 'Identifier is already owned by another program' }))).toBe('Learning source versions “Field guide v2” already belongs to another course on this computer.');
    expect(blockedReason(conflict('learning_practical_runs', { incomingTitle: 'learning_practical_runs', existingTitle: 'Identifier is already owned by another program' }))).toBe('Learning practical runs already belongs to another course on this computer.');
  });

  it("uses the backend's own reason for a build, preparation or history block, with a fallback", () => {
    expect(blockedReason(conflict('program_jobs', { existingTitle: 'This course is still being prepared.' }))).toBe('This course is still being prepared.');
    expect(blockedReason(conflict('program_jobs', { existingTitle: ' ' }))).toBe('The copy on this computer is still being prepared. Wait for it to finish before replacing it.');
  });
});
