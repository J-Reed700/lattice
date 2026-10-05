import { describe, expect, it } from 'vitest';

import { nextCourseStep } from '@/features/learning/curriculum/courseProgress';
import type { LearningAssessmentWorkspaceDto, LearningPlanDto, LearningPracticeWorkspaceDto, LearningProgramDto } from '@/lib/bindings';


const program = { summary: { id: 'p', currentLessonId: 'l' }, modules: [{ id: 'm', lessons: [{ id: 'l', title: 'Accumulate a total', preparation: 'ready', completed: false, blocks: [{ kind: 'guided_practice' }] }] }] } as LearningProgramDto;
const assessment = { programId: 'p', outcomes: [{ id: 'o', moduleId: 'm', lessonId: 'l', title: 'Accumulate', description: 'Track a running total', ordinal: 0, createdAt: 0 }], forms: [], blueprints: [], evidence: [], followUps: [] } as LearningAssessmentWorkspaceDto;

describe('course next steps', () => {
  it('resumes a saved guided attempt instead of offering another exercise', () => {
    const practice = { sessions: [{ id: 'attempt', lessonId: 'l', lessonTitle: 'Saved task', taskKind: 'guided', status: 'active', updatedAt: 20 }] } as LearningPracticeWorkspaceDto;
    expect(nextCourseStep(program, practice, assessment)).toMatchObject({ kind: 'guided', sessionId: 'attempt', lessonId: 'l' });
  });
  it('resumes the more recently started assessment by its exact form ID', () => {
    const forms = { ...assessment, forms: [{ id: 'form', title: 'Checkpoint', status: 'active', createdAt: 30 }] } as LearningAssessmentWorkspaceDto;
    expect(nextCourseStep(program, undefined, forms)).toMatchObject({ kind: 'assessment', formId: 'form' });
  });
  it('moves from guided work to independent work without marking a lesson complete', () => {
    const practice = { sessions: [{ lessonId: 'l', taskKind: 'guided', status: 'submitted' }] } as LearningPracticeWorkspaceDto;
    expect(nextCourseStep(program, practice, assessment).kind).toBe('assignment');
    expect(program.modules[0].lessons[0].completed).toBe(false);
  });
  it('uses a missed-outcome follow-up and stops suggesting it after newer application evidence', () => {
    const evidence = { ...assessment, followUps: [{ id: 'f', outcomeId: 'o', actionRef: 'l', actionKind: 'practice', status: 'pending', createdAt: 10, explanation: 'Your loop overwrote the previous total.' }] } as LearningAssessmentWorkspaceDto;
    expect(nextCourseStep(program, undefined, evidence)).toMatchObject({ kind: 'assignment', reason: 'Your loop overwrote the previous total.' });
    evidence.evidence = [{ outcomeId: 'o', observedAt: 20, result: 'observed' }] as LearningAssessmentWorkspaceDto['evidence'];
    expect(nextCourseStep(program, undefined, evidence).kind).toBe('guided');
  });
  it('turns placement findings into a visible study suggestion without skipping lessons', () => {
    const plan = { acceptedRevision: { modules: [{ outcomeIds: ['o'], lessons: [{ id: 'l' }] }] }, latestDiagnostic: { status: 'submitted', submittedAt: 10, findings: [{ outcomeId: 'o', signal: 'needs_practice', feedback: 'Practice the accumulator update.' }] } } as LearningPlanDto;
    expect(nextCourseStep(program, undefined, assessment, plan)).toMatchObject({ kind: 'lesson', lessonId: 'l', reason: 'Practice the accumulator update.' });
    expect(program.modules[0].lessons[0].completed).toBe(false);
    const newer = { ...assessment, evidence: [{ outcomeId: 'o', observedAt: 20, result: 'observed' }] } as LearningAssessmentWorkspaceDto;
    expect(nextCourseStep(program, undefined, newer, plan).kind).toBe('guided');
  });
  it('does not infer skill from self-reported completion', () => {
    const finished = structuredClone(program); finished.modules[0].lessons[0].completed = true;
    expect(nextCourseStep(finished, undefined, assessment).kind).toBe('assessment');
  });
});
