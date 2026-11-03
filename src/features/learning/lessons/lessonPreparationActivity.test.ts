import { describe, expect, it } from 'vitest';

import { callsLine, preparationState, preparationSummary, preparingLine, statementsLine } from '@/features/learning/lessons/lessonPreparationActivity';
import type { LearningGenerationJob } from '@/lib/bindings';

const job = (overrides: Partial<LearningGenerationJob> = {}): LearningGenerationJob => ({
  id: 'job-1', programId: 'program-1', operationId: 'operation-1', kind: 'lesson_preparation',
  payloadSha256: 'hash', baseRevisionNumber: 1, status: 'running', progressCompleted: 0,
  activity: {
    phase: 'review', phaseStartedAt: 1, lastActivityAt: 1, lastCheckpointAt: null, lessonTitle: 'Boiling', modelName: null,
    modelRunning: true, responseCharacters: 0, modelAttempt: 1, verificationPass: 0,
    checksCompleted: 0, checksTotal: 0, checksReused: 0, newReferences: null, claims: null,
    recentSteps: [],
  },
  progressTotal: 1, progressMessage: '', resultId: null, error: null, retryOfJobId: null, createdAt: 1, startedAt: 1, finishedAt: null,
  ...overrides,
});

const line = (value?: LearningGenerationJob, pending = false, error?: string) => preparationSummary(value, preparationState(value, pending), error);

describe('Lesson preparation in one line', () => {
  it('names the lesson and its current step, or how it stopped', () => {
    expect(line(job())).toBe('Preparing lesson: Boiling · Reviewing teaching and assessments');
    expect(line(job({ status: 'pending' }))).toBe('Preparing lesson: Boiling · Queued');
    expect(line(job({ status: 'pending', error: 'The model is unavailable.' }))).toBe('Lesson preparation will resume automatically: Boiling');
    expect(line(job({ status: 'cancelled' }))).toBe('Lesson preparation paused: Boiling');
    expect(line(job({ status: 'failed' }))).toBe('Lesson preparation failed: Boiling');
    expect(line(job({ status: 'completed', resultId: 'needs_review:lesson-1' }))).toBe('Lesson waiting for your review: Boiling');
    expect(line(job({ activity: null }))).toBe('Preparing lesson · Starting');
  });

  it('is empty with no job, nothing pending and no error', () => {
    expect(line(undefined)).toBeNull();
    expect(line(job({ status: 'completed' }))).toBeNull();
    expect(line(undefined, true)).toBe('Preparing lesson · Starting');
    expect(line(undefined, false, 'The request could not be saved.')).toBe('Lesson preparation failed');
  });
});

describe('The plan of a lesson\'s statements', () => {
  it('says what the statements were sorted into', () => {
    expect(statementsLine({ sourced: 36, reasoning: 29, common: 8 })).toBe('73 statements: 36 to check against their sources, 29 to check for consistency, 8 common knowledge');
    expect(statementsLine({ sourced: 1, reasoning: 0, common: 0 })).toBe('1 statement: 1 to check against their sources, 0 to check for consistency, 0 common knowledge');
  });

  it('puts the calls made beside the most there can be, and has nothing to say before the first call', () => {
    const activity = (modelCalls?: number, modelCallsCeiling?: number | null) => ({ ...job().activity!, modelCalls, modelCallsCeiling });
    expect(callsLine(activity(112, 278))).toBe('112 calls so far, at most 278');
    expect(callsLine(activity(1, 278))).toBe('1 call so far, at most 278');
    expect(callsLine(activity(2, null))).toBe('2 calls so far');
    expect(callsLine(activity(0, 278))).toBe('0 calls so far, at most 278');
    expect(callsLine(activity())).toBeNull();
    expect(callsLine(activity(0, null))).toBeNull();
  });
});

describe('The row of a lesson being prepared', () => {
  it('counts the claims checked while they are checked, and names the phase otherwise', () => {
    const activity = job().activity!;
    expect(preparingLine(null)).toBe('Preparing');
    expect(preparingLine(undefined)).toBe('Preparing');
    expect(preparingLine({ ...activity, phase: 'evidence', checksCompleted: 40, checksTotal: 44 })).toBe('Preparing · 40 of 44 checked');
    expect(preparingLine({ ...activity, phase: 'evidence' })).toBe('Preparing · Verifying factual claims');
    expect(preparingLine({ ...activity, phase: 'repair', checksCompleted: 44, checksTotal: 44 })).toBe('Preparing · Repairing the draft');
  });
});
