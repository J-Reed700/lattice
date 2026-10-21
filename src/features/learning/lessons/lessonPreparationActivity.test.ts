import { describe, expect, it } from 'vitest';

import { preparationState, preparationSummary } from '@/features/learning/lessons/lessonPreparationActivity';
import type { LearningGenerationJob } from '@/lib/bindings';

const job = (overrides: Partial<LearningGenerationJob> = {}): LearningGenerationJob => ({
  id: 'job-1', programId: 'program-1', operationId: 'operation-1', kind: 'lesson_preparation',
  payloadSha256: 'hash', baseRevisionNumber: 1, status: 'running', progressCompleted: 0,
  activity: {
    phase: 'review', phaseStartedAt: 1, lastActivityAt: 1, lastCheckpointAt: null, lessonTitle: 'Boiling', modelName: null,
    modelRunning: true, responseCharacters: 0, modelAttempt: 1, verificationPass: 0,
    checksCompleted: 0, checksTotal: 0, checksReused: 0, checksUnresolved: 0, checksSentBack: 0, newReferences: null, modelChecksTotal: null, deeperEvidence: null, claims: null,
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
