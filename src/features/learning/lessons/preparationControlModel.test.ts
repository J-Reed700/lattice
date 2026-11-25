import { describe, expect, it } from 'vitest';

import {
  buildPreparationControlModel,
  filterPreparationRows,
  getBulkActionEligibility,
  type PreparationJobWithRank,
} from '@/features/learning/lessons/preparationControlModel';
import type {
  LearningGenerationActivity,
  LearningLessonDto,
  LearningProgramDto,
} from '@/lib/bindings';

function makeLesson(
  id: string,
  title: string,
  preparation: 'outline' | 'ready' = 'outline',
): LearningLessonDto {
  return {
    id,
    title,
    objective: `Learn ${title}`,
    estimatedMinutes: 20,
    preparation,
    blocks: [],
    completed: false,
  } as unknown as LearningLessonDto;
}

function makeProgram(modules: Array<{ id: string; title: string; lessons: LearningLessonDto[] }>): LearningProgramDto {
  return {
    summary: {
      id: 'program-1',
      title: 'Systems Engineering',
      goal: 'Master systems',
      status: 'active',
      revision: 12,
      moduleCount: modules.length,
      lessonCount: modules.reduce((sum, mod) => sum + mod.lessons.length, 0),
      completedLessons: 0,
      currentLessonId: modules[0]?.lessons[0]?.id ?? null,
      createdAt: 1,
    },
    priorKnowledge: '',
    minutesPerSession: 30,
    modelName: 'test-model',
    outlineReview: null,
    sources: [],
    modules: modules.map((mod) => ({
      ...mod,
      summary: `${mod.title} summary`,
      outcomes: [],
      prerequisiteModuleIds: [],
      project: null,
    })),
  };
}

function makeActivity(overrides: Partial<LearningGenerationActivity> = {}): LearningGenerationActivity {
  return {
    phase: 'evidence',
    phaseStartedAt: 1_000,
    lastActivityAt: 2_000,
    lastCheckpointAt: 1_500,
    lessonTitle: null,
    modelName: 'test-model',
    modelRunning: true,
    responseCharacters: 100,
    modelAttempt: 1,
    verificationPass: 1,
    checksCompleted: 12,
    checksTotal: 30,
    checksReused: 4,
    newReferences: null,
    statements: null,
    claims: null,
    recentSteps: [],
    ...overrides,
  };
}

function makeJob(overrides: Partial<PreparationJobWithRank> = {}): PreparationJobWithRank {
  return {
    id: 'job-1',
    programId: 'program-1',
    operationId: 'op-1',
    kind: 'lesson_preparation',
    payloadSha256: 'hash',
    baseRevisionNumber: 12,
    status: 'pending',
    progressCompleted: 0,
    progressTotal: 1,
    progressMessage: 'Queued',
    activity: null,
    resultId: null,
    error: null,
    errorCode: null,
    endedByUpdate: false,
    lessonIds: [],
    heldLessonIds: [],
    retryOfJobId: null,
    createdAt: 1_000,
    startedAt: null,
    finishedAt: null,
    ...overrides,
  };
}

describe('preparationControlModel', () => {
  it('derives all 7 lesson statuses and course-wide tallies accurately', () => {
    const program = makeProgram([
      {
        id: 'mod-1',
        title: 'Foundations',
        lessons: [
          makeLesson('l-ready', 'Ready Lesson', 'ready'),
          makeLesson('l-running', 'Running Lesson'),
          makeLesson('l-queued', 'Queued Lesson'),
          makeLesson('l-paused', 'Paused Lesson'),
        ],
      },
      {
        id: 'mod-2',
        title: 'Verification',
        lessons: [
          makeLesson('l-failed', 'Failed Lesson'),
          makeLesson('l-interrupted', 'Interrupted Lesson'),
          makeLesson('l-review', 'Review Lesson'),
          makeLesson('l-fresh', 'Fresh Lesson'),
        ],
      },
    ]);

    const jobs: PreparationJobWithRank[] = [
      makeJob({
        id: 'job-running',
        status: 'running',
        lessonIds: ['l-running'],
        createdAt: 100,
        startedAt: 120,
        progressMessage: 'Checking factual statements',
        activity: makeActivity({ phase: 'evidence', checksCompleted: 18, checksTotal: 40, checksReused: 6 }),
      }),
      makeJob({
        id: 'job-queued',
        status: 'pending',
        lessonIds: ['l-queued'],
        createdAt: 200,
        progressMessage: 'Waiting in queue',
      }),
      makeJob({
        id: 'job-paused',
        status: 'cancelled',
        lessonIds: ['l-paused'],
        createdAt: 150,
        activity: makeActivity({ phase: 'writing', checksCompleted: 0, checksTotal: 0, checksReused: 0 }),
      }),
      makeJob({
        id: 'job-failed',
        status: 'failed',
        lessonIds: ['l-failed'],
        createdAt: 160,
        error: 'Model connection timed out',
        errorCode: 'NETWORK_ERROR',
      }),
      makeJob({
        id: 'job-interrupted',
        status: 'interrupted',
        lessonIds: ['l-interrupted'],
        createdAt: 170,
        error: 'App quit mid-run',
      }),
      makeJob({
        id: 'job-review',
        status: 'completed',
        lessonIds: ['l-review'],
        heldLessonIds: ['l-review'],
        createdAt: 180,
      }),
    ];

    const model = buildPreparationControlModel(program, jobs);
    const byId = Object.fromEntries(model.rows.map((r) => [r.lesson.id, r]));

    expect(byId['l-ready'].status).toBe('ready');
    expect(byId['l-running'].status).toBe('running');
    expect(byId['l-running'].ordinal).toBe('1.2');
    expect(byId['l-running'].checksCompleted).toBe(18);
    expect(byId['l-running'].checksTotal).toBe(40);
    expect(byId['l-running'].savedWorkHint).toBe('6 saved checks kept');
    expect(byId['l-queued'].status).toBe('queued');
    expect(byId['l-paused'].status).toBe('paused');
    expect(byId['l-paused'].savedWorkHint).toBe('Checkpoints saved');
    expect(byId['l-failed'].status).toBe('failed');
    expect(byId['l-failed'].errorMessage).toBe('Model connection timed out');
    expect(byId['l-interrupted'].status).toBe('failed');
    expect(byId['l-review'].status).toBe('needs_review');
    expect(byId['l-fresh'].status).toBe('not_started');

    expect(model.counts).toEqual({
      total: 8,
      running: 1,
      queued: 1,
      active: 2,
      paused: 1,
      failed: 2,
      needs_review: 1,
      ready: 1,
      not_started: 1,
    });
  });

  it('ignores endedByUpdate and superseded jobs, prefers active jobs, and keeps ready lessons ready', () => {
    const program = makeProgram([
      {
        id: 'mod-1',
        title: 'Module 1',
        lessons: [
          makeLesson('l-1', 'Ready with old failure', 'ready'),
          makeLesson('l-2', 'Retried active lesson'),
          makeLesson('l-3', 'Ended by update lesson'),
          makeLesson('l-4', 'New active job over old cancelled'),
        ],
      },
    ]);

    const jobs: PreparationJobWithRank[] = [
      // Historical failure + held job on an already-ready lesson
      makeJob({ id: 'old-fail-1', status: 'failed', lessonIds: ['l-1'], heldLessonIds: ['l-1'], error: 'Old error', createdAt: 50 }),
      // Superseded failed job -> replaced by paused retry
      makeJob({ id: 'superseded-fail', status: 'failed', lessonIds: ['l-2'], error: 'Boom', createdAt: 100 }),
      makeJob({ id: 'retry-paused', status: 'cancelled', retryOfJobId: 'superseded-fail', lessonIds: ['l-2'], createdAt: 200 }),
      // Ended by update
      makeJob({ id: 'stale-update', status: 'failed', lessonIds: ['l-3'], endedByUpdate: true, createdAt: 300 }),
      // Old cancelled + newer pending job without retryOfJobId
      makeJob({ id: 'old-cancel', status: 'cancelled', lessonIds: ['l-4'], createdAt: 100 }),
      makeJob({ id: 'new-pending', status: 'pending', lessonIds: ['l-4'], createdAt: 400 }),
      // Non-lesson_preparation kind should be ignored
      makeJob({ id: 'quiz-job', kind: 'quiz_bank', status: 'running', lessonIds: ['l-3'], createdAt: 500 }),
    ];

    const model = buildPreparationControlModel(program, jobs);
    const byId = Object.fromEntries(model.rows.map((r) => [r.lesson.id, r]));

    expect(byId['l-1'].status).toBe('ready');
    expect(byId['l-1'].job).toBeNull();
    expect(byId['l-2'].status).toBe('paused');
    expect(byId['l-2'].job?.id).toBe('retry-paused');
    expect(byId['l-3'].status).toBe('not_started');
    expect(byId['l-4'].status).toBe('queued');
    expect(byId['l-4'].job?.id).toBe('new-pending');
  });

  it('orders active queue by rank ?? Infinity, then createdAt and assigns 1-based queueIndex', () => {
    const program = makeProgram([
      {
        id: 'mod-1',
        title: 'Queue Order',
        lessons: [
          makeLesson('l-a', 'Lesson A'),
          makeLesson('l-b', 'Lesson B'),
          makeLesson('l-c', 'Lesson C'),
          makeLesson('l-d', 'Lesson D'),
        ],
      },
    ]);

    const jobs: PreparationJobWithRank[] = [
      makeJob({ id: 'j-a', status: 'pending', lessonIds: ['l-a'], createdAt: 100 }),
      makeJob({ id: 'j-b', status: 'pending', lessonIds: ['l-b'], createdAt: 300, rank: 2 }),
      makeJob({ id: 'j-c', status: 'pending', lessonIds: ['l-c'], createdAt: 400, rank: 1 }),
      makeJob({ id: 'j-d', status: 'running', lessonIds: ['l-d'], createdAt: 50 }),
    ];

    const model = buildPreparationControlModel(program, jobs);
    const byId = Object.fromEntries(model.rows.map((r) => [r.lesson.id, r]));

    // rank 1 (j-c), rank 2 (j-b), rank Infinity + createdAt 50 (j-d), rank Infinity + createdAt 100 (j-a)
    expect(byId['l-c'].queueIndex).toBe(1);
    expect(byId['l-b'].queueIndex).toBe(2);
    expect(byId['l-d'].queueIndex).toBe(3);
    expect(byId['l-a'].queueIndex).toBe(4);
  });

  it('filters by status pill and searches by lesson or module title', () => {
    const program = makeProgram([
      {
        id: 'mod-1',
        title: 'Memory Models',
        lessons: [
          makeLesson('l-1', 'Cache Coherence'),
          makeLesson('l-2', 'Atomics', 'ready'),
        ],
      },
      {
        id: 'mod-2',
        title: 'Networking',
        lessons: [
          makeLesson('l-3', 'TCP Congestion'),
        ],
      },
    ]);

    const jobs: PreparationJobWithRank[] = [
      makeJob({ id: 'j-1', status: 'running', lessonIds: ['l-1'], createdAt: 10 }),
      makeJob({ id: 'j-3', status: 'pending', lessonIds: ['l-3'], createdAt: 20 }),
    ];

    const model = buildPreparationControlModel(program, jobs);

    expect(filterPreparationRows(model.rows, 'active', '').map((r) => r.lesson.id)).toEqual(['l-1', 'l-3']);
    expect(filterPreparationRows(model.rows, 'ready', '').map((r) => r.lesson.id)).toEqual(['l-2']);
    expect(filterPreparationRows(model.rows, 'all', 'memory').map((r) => r.lesson.id)).toEqual(['l-1', 'l-2']);
    expect(filterPreparationRows(model.rows, 'active', 'congestion').map((r) => r.lesson.id)).toEqual(['l-3']);
  });

  it('computes bulk action eligibility and deduplicates shared multi-lesson jobs', () => {
    const program = makeProgram([
      {
        id: 'mod-1',
        title: 'Module 1',
        lessons: [
          makeLesson('l-1', 'Not started'),
          makeLesson('l-2', 'Running 1'),
          makeLesson('l-3', 'Queued 2 (shared job)'),
          makeLesson('l-4', 'Paused'),
          makeLesson('l-5', 'Failed'),
          makeLesson('l-6', 'Review'),
          makeLesson('l-7', 'Ready', 'ready'),
        ],
      },
    ]);

    const jobs: PreparationJobWithRank[] = [
      makeJob({ id: 'j-shared', status: 'running', lessonIds: ['l-2', 'l-3'], createdAt: 10 }),
      makeJob({ id: 'j-paused', status: 'cancelled', lessonIds: ['l-4'], createdAt: 20 }),
      makeJob({ id: 'j-failed', status: 'failed', lessonIds: ['l-5'], createdAt: 30 }),
      makeJob({ id: 'j-review', status: 'completed', lessonIds: ['l-6'], heldLessonIds: ['l-6'], createdAt: 40 }),
    ];

    const model = buildPreparationControlModel(program, jobs);
    const eligibility = getBulkActionEligibility(
      model.rows,
      new Set(['l-1', 'l-2', 'l-3', 'l-4', 'l-5', 'l-6', 'l-7']),
    );

    expect(eligibility.selectedCount).toBe(7);
    expect(eligibility.preparableIds).toEqual(['l-1']);
    expect(eligibility.pausableJobs.map((j) => j.id)).toEqual(['j-shared']);
    expect(eligibility.resumableJobs.map((j) => j.id)).toEqual(['j-paused']);
    expect(eligibility.retryableJobs.map((j) => j.id)).toEqual(['j-failed']);
    expect(eligibility.resettableIds).toEqual(['l-2', 'l-3', 'l-4', 'l-5', 'l-6', 'l-7']);
  });

  it('handles partial activity objects without crashing when phase is omitted', () => {
    const program = makeProgram([
      {
        id: 'mod-1',
        title: 'Module 1',
        lessons: [makeLesson('l-1', 'Partial activity')],
      },
    ]);
    const jobs = [
      makeJob({
        id: 'j-partial',
        status: 'failed',
        lessonIds: ['l-1'],
        error: 'Timed out',
        activity: { lessonTitle: 'Partial activity' } as unknown as LearningGenerationActivity,
      }),
    ];

    const model = buildPreparationControlModel(program, jobs);
    expect(model.rows[0].status).toBe('failed');
    expect(model.rows[0].phaseLabel).toBeNull();
  });
});
