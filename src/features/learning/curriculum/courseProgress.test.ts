import { describe, expect, it } from 'vitest';

import { nextCourseStep } from '@/features/learning/curriculum/courseProgress';
import { quizOverview, quizStatus } from '@/features/learning/quizzes/quizTestData';
import type { LearningPlanDto, LearningPracticeWorkspaceDto, LearningProgramDto, LearningQuizOverviewDto } from '@/lib/bindings';


const program = { summary: { id: 'p', currentLessonId: 'l' }, modules: [{ id: 'm', lessons: [{ id: 'l', title: 'Accumulate a total', preparation: 'ready', completed: false, blocks: [{ kind: 'guided_practice' }] }] }] } as LearningProgramDto;
const assessment: LearningQuizOverviewDto = { ...quizOverview({ programId: 'p', modules: [] }), outcomes: [{ id: 'o', moduleId: 'm', lessonId: 'l', title: 'Accumulate', description: 'Track a running total', ordinal: 0, createdAt: 0 }] };

describe('course next steps', () => {
  it('resumes a saved guided attempt instead of offering another exercise', () => {
    const practice = { sessions: [{ id: 'attempt', lessonId: 'l', lessonTitle: 'Saved task', taskKind: 'guided', status: 'active', updatedAt: 20 }] } as LearningPracticeWorkspaceDto;
    expect(nextCourseStep(program, practice, assessment)).toMatchObject({ kind: 'guided', sessionId: 'attempt', lessonId: 'l' });
  });
  it('resumes a paused quiz by its exact form ID', () => {
    const paused = { ...assessment, modules: [{ moduleId: 'm', test: quizStatus({ scope: { kind: 'module', id: 'm' } }), lessons: [{ lessonId: 'l', quiz: quizStatus({ scope: { kind: 'lesson', id: 'l' }, activeFormId: 'form' }) }] }] };
    expect(nextCourseStep(program, undefined, paused)).toMatchObject({ kind: 'assessment', formId: 'form', title: 'Resume your quiz' });
  });
  it('suggests the lesson quiz after independent practice, until it is passed, and never completes the lesson', () => {
    const practice = { sessions: [{ lessonId: 'l', taskKind: 'guided', status: 'submitted' }, { lessonId: 'l', taskKind: 'independent', status: 'submitted' }] } as LearningPracticeWorkspaceDto;
    const withQuiz = (verdict: 'not_taken' | 'passed' | 'pending') => ({ ...assessment, modules: [{ moduleId: 'm', test: quizStatus({ scope: { kind: 'module', id: 'm' } }), lessons: [{ lessonId: 'l', quiz: quizStatus({ scope: { kind: 'lesson', id: 'l' }, verdict }) }] }] });
    expect(nextCourseStep(program, practice, withQuiz('not_taken'))).toMatchObject({ kind: 'quiz', title: 'Take the lesson quiz', lessonId: 'l' });
    expect(nextCourseStep(program, practice, withQuiz('passed')).kind).toBe('lesson');
    expect(nextCourseStep(program, practice, withQuiz('pending')).kind).toBe('lesson');
    expect(program.modules[0].lessons[0].completed).toBe(false);
  });
  it('moves from guided work to independent work without marking a lesson complete', () => {
    const practice = { sessions: [{ lessonId: 'l', taskKind: 'guided', status: 'submitted' }] } as LearningPracticeWorkspaceDto;
    expect(nextCourseStep(program, practice, assessment).kind).toBe('assignment');
    expect(program.modules[0].lessons[0].completed).toBe(false);
  });
  it('uses a missed-outcome follow-up and stops suggesting it after newer application evidence', () => {
    const evidence = { ...assessment, followUps: [{ id: 'f', outcomeId: 'o', actionRef: 'l', actionKind: 'practice', status: 'pending', createdAt: 10, explanation: 'Your loop overwrote the previous total.' }] } as LearningQuizOverviewDto;
    expect(nextCourseStep(program, undefined, evidence)).toMatchObject({ kind: 'assignment', reason: 'Your loop overwrote the previous total.' });
    evidence.evidence = [{ outcomeId: 'o', observedAt: 20, result: 'observed' }] as LearningQuizOverviewDto['evidence'];
    expect(nextCourseStep(program, undefined, evidence).kind).toBe('guided');
  });
  it('turns placement findings into a visible study suggestion without skipping lessons', () => {
    const plan = { acceptedRevision: { modules: [{ outcomeIds: ['o'], lessons: [{ id: 'l' }] }] }, latestDiagnostic: { status: 'submitted', submittedAt: 10, findings: [{ outcomeId: 'o', signal: 'needs_practice', feedback: 'Practice the accumulator update.' }] } } as LearningPlanDto;
    expect(nextCourseStep(program, undefined, assessment, plan)).toMatchObject({ kind: 'lesson', lessonId: 'l', reason: 'Practice the accumulator update.' });
    expect(program.modules[0].lessons[0].completed).toBe(false);
    const newer = { ...assessment, evidence: [{ outcomeId: 'o', observedAt: 20, result: 'observed' }] } as LearningQuizOverviewDto;
    expect(nextCourseStep(program, undefined, newer, plan).kind).toBe('guided');
  });
  it('does not infer skill from self-reported completion', () => {
    const finished = structuredClone(program); finished.modules[0].lessons[0].completed = true;
    expect(nextCourseStep(finished, undefined, assessment).kind).toBe('assessment');
  });
});
