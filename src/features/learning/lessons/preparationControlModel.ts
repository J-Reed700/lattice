import {
  PREPARATION_PHASES,
  shownPhase,
  stageText as formatStageText,
} from '@/features/learning/lessons/lessonPreparationActivity';
import { currentPreparationJobs, heldLessonIds } from '@/features/learning/lessons/review/claimReviewPresentation';
import type {
  ErrorCode,
  LearningGenerationJob,
  LearningLessonDto,
  LearningModuleDto,
  LearningProgramDto,
} from '@/lib/bindings';

export type PreparationJobWithRank = LearningGenerationJob & {
  rank?: number | null;
};

export type LessonPreparationControlStatus =
  | 'running'
  | 'queued'
  | 'paused'
  | 'failed'
  | 'needs_review'
  | 'ready'
  | 'not_started';

export type PreparationFilter =
  | 'all'
  | 'active'
  | 'paused'
  | 'failed'
  | 'needs_review'
  | 'not_started'
  | 'ready';

export type LessonPreparationRow = {
  lesson: LearningLessonDto;
  module: LearningModuleDto;
  moduleIndex: number;
  lessonIndex: number;
  ordinal: string;
  status: LessonPreparationControlStatus;
  job: PreparationJobWithRank | null;
  queueIndex: number | null;
  reconnecting: boolean;
  waitingForModel: boolean;
  phaseLabel: string | null;
  stageText: string | null;
  checksCompleted: number;
  checksTotal: number;
  checksReused: number;
  lastCheckpointAt: number | null;
  savedWorkHint: string | null;
  errorMessage: string | null;
  errorCode: ErrorCode | null;
  startedAt: number | null;
};

export type PreparationStatusCounts = {
  total: number;
  running: number;
  queued: number;
  active: number;
  paused: number;
  failed: number;
  needs_review: number;
  ready: number;
  not_started: number;
};

export type ModulePreparationGroup = {
  module: LearningModuleDto;
  moduleIndex: number;
  rows: LessonPreparationRow[];
  readyCount: number;
  totalCount: number;
  notStartedIds: string[];
};

export type BulkActionEligibility = {
  selectedCount: number;
  preparableIds: string[];
  pausableJobs: PreparationJobWithRank[];
  resumableJobs: PreparationJobWithRank[];
  retryableJobs: PreparationJobWithRank[];
  resettableIds: string[];
};

export type PreparationControlModel = {
  rows: LessonPreparationRow[];
  modules: ModulePreparationGroup[];
  counts: PreparationStatusCounts;
  queue: PreparationJobWithRank[];
  globalPausableJobs: PreparationJobWithRank[];
  globalResumableJobs: PreparationJobWithRank[];
  globalRetryableJobs: PreparationJobWithRank[];
  globalNotStartedIds: string[];
};

function compareQueueJobs(a: PreparationJobWithRank, b: PreparationJobWithRank): number {
  const rankA = a.rank ?? Number.POSITIVE_INFINITY;
  const rankB = b.rank ?? Number.POSITIVE_INFINITY;
  if (rankA !== rankB) return rankA - rankB;
  if (a.createdAt !== b.createdAt) return a.createdAt - b.createdAt;
  return a.id.localeCompare(b.id);
}

function jobCoversLesson(job: LearningGenerationJob, lessonId: string): boolean {
  return Boolean(job.lessonIds?.includes(lessonId) || heldLessonIds(job).includes(lessonId));
}

function deduplicateJobs(jobs: Array<PreparationJobWithRank | null>): PreparationJobWithRank[] {
  const seen = new Set<string>();
  const result: PreparationJobWithRank[] = [];
  for (const job of jobs) {
    if (!job || seen.has(job.id)) continue;
    seen.add(job.id);
    result.push(job);
  }
  return result;
}

function deriveSavedWorkHint(
  status: LessonPreparationControlStatus,
  job: PreparationJobWithRank | null,
): string | null {
  if (!job) return null;
  const reused = job.activity?.checksReused ?? 0;
  if (reused > 0) {
    return `${reused} saved ${reused === 1 ? 'check' : 'checks'} kept`;
  }
  if (job.activity?.lastCheckpointAt != null) {
    return 'Checkpoints saved';
  }
  if (status === 'paused' || (status === 'failed' && job.activity != null)) {
    return 'Draft and checkpoints saved';
  }
  return null;
}

function derivePhaseLabel(
  status: LessonPreparationControlStatus,
  job: PreparationJobWithRank | null,
  reconnecting: boolean,
): string | null {
  if (!job) return null;
  if (reconnecting) return 'Waiting to reconnect';
  if (job.activity?.phase && job.activity.phase in PREPARATION_PHASES) {
    return shownPhase(job.activity).title;
  }
  if (status === 'running') return 'Starting preparation';
  if (status === 'queued') return 'Queued';
  if (status === 'paused') return 'Paused';
  if (status === 'needs_review') return 'Waiting for your review';
  return null;
}

function deriveStageText(
  job: PreparationJobWithRank | null,
  reconnecting: boolean,
): string | null {
  if (!job) return null;
  if (reconnecting) return 'Retries automatically while Lattice is open';
  if (job.activity) {
    return formatStageText(job.activity, job.progressMessage) || null;
  }
  return job.progressMessage || null;
}

function resolveLessonStatusAndJob(
  lesson: LearningLessonDto,
  sortedQueue: PreparationJobWithRank[],
  currentJobs: PreparationJobWithRank[],
): { status: LessonPreparationControlStatus; job: PreparationJobWithRank | null; queueIndex: number | null } {
  if (lesson.preparation === 'ready') {
    return { status: 'ready', job: null, queueIndex: null };
  }

  const activeCandidates = sortedQueue.filter((job) => job.lessonIds?.includes(lesson.id));
  const runningJob = activeCandidates.find((job) => job.status === 'running');
  const activeJob = runningJob ?? activeCandidates[0] ?? null;
  if (activeJob) {
    const queueIndex = sortedQueue.indexOf(activeJob) + 1;
    return {
      status: activeJob.status === 'running' ? 'running' : 'queued',
      job: activeJob,
      queueIndex,
    };
  }

  const stoppedJobs = currentJobs
    .filter((job) => job.status !== 'pending' && job.status !== 'running' && jobCoversLesson(job, lesson.id))
    .sort((a, b) => b.createdAt - a.createdAt || b.id.localeCompare(a.id));

  for (const candidate of stoppedJobs) {
    if (heldLessonIds(candidate).includes(lesson.id)) {
      return { status: 'needs_review', job: candidate, queueIndex: null };
    }
    if (candidate.status === 'cancelled') {
      return { status: 'paused', job: candidate, queueIndex: null };
    }
    if (candidate.status === 'failed' || candidate.status === 'interrupted') {
      return { status: 'failed', job: candidate, queueIndex: null };
    }
  }

  return { status: 'not_started', job: null, queueIndex: null };
}

export function buildPreparationControlModel(
  program: LearningProgramDto,
  planJobs: PreparationJobWithRank[],
): PreparationControlModel {
  const currentJobs = currentPreparationJobs(planJobs).filter(
    (job): job is PreparationJobWithRank => !job.endedByUpdate,
  );
  const queue = currentJobs
    .filter((job) => job.status === 'pending' || job.status === 'running')
    .sort(compareQueueJobs);

  const counts: PreparationStatusCounts = {
    total: 0,
    running: 0,
    queued: 0,
    active: 0,
    paused: 0,
    failed: 0,
    needs_review: 0,
    ready: 0,
    not_started: 0,
  };

  const rows: LessonPreparationRow[] = [];
  const modules: ModulePreparationGroup[] = [];

  program.modules.forEach((module, moduleIndex) => {
    const moduleRows: LessonPreparationRow[] = [];
    let readyCount = 0;
    const notStartedIds: string[] = [];

    module.lessons.forEach((lesson, lessonIndex) => {
      const { status, job, queueIndex } = resolveLessonStatusAndJob(lesson, queue, currentJobs);
      const reconnecting = status === 'queued' && Boolean(job?.error);
      const waitingForModel = Boolean(job?.activity?.waitingForModel);
      const checksCompleted = job?.activity?.checksCompleted ?? 0;
      const checksTotal = job?.activity?.checksTotal ?? 0;
      const checksReused = job?.activity?.checksReused ?? 0;
      const lastCheckpointAt = job?.activity?.lastCheckpointAt ?? null;

      const row: LessonPreparationRow = {
        lesson,
        module,
        moduleIndex,
        lessonIndex,
        ordinal: `${moduleIndex + 1}.${lessonIndex + 1}`,
        status,
        job,
        queueIndex,
        reconnecting,
        waitingForModel,
        phaseLabel: derivePhaseLabel(status, job, reconnecting),
        stageText: deriveStageText(job, reconnecting),
        checksCompleted,
        checksTotal,
        checksReused,
        lastCheckpointAt,
        savedWorkHint: deriveSavedWorkHint(status, job),
        errorMessage: job?.error ?? null,
        errorCode: job?.errorCode ?? null,
        startedAt: job ? (job.startedAt ?? job.createdAt) : null,
      };

      counts.total += 1;
      counts[status] += 1;
      if (status === 'running' || status === 'queued') {
        counts.active += 1;
      }
      if (status === 'ready') {
        readyCount += 1;
      } else if (status === 'not_started') {
        notStartedIds.push(lesson.id);
      }

      rows.push(row);
      moduleRows.push(row);
    });

    modules.push({
      module,
      moduleIndex,
      rows: moduleRows,
      readyCount,
      totalCount: moduleRows.length,
      notStartedIds,
    });
  });

  return {
    rows,
    modules,
    counts,
    queue,
    globalPausableJobs: deduplicateJobs(
      rows.filter((row) => row.status === 'running' || row.status === 'queued').map((row) => row.job),
    ),
    globalResumableJobs: deduplicateJobs(
      rows.filter((row) => row.status === 'paused').map((row) => row.job),
    ),
    globalRetryableJobs: deduplicateJobs(
      rows.filter((row) => row.status === 'failed').map((row) => row.job),
    ),
    globalNotStartedIds: rows.filter((row) => row.status === 'not_started').map((row) => row.lesson.id),
  };
}

export function matchesPreparationFilter(
  row: LessonPreparationRow,
  filter: PreparationFilter,
  searchQuery: string,
): boolean {
  const statusMatch =
    filter === 'all'
      ? true
      : filter === 'active'
        ? row.status === 'running' || row.status === 'queued'
        : row.status === filter;
  if (!statusMatch) return false;

  const query = searchQuery.trim().toLowerCase();
  if (!query) return true;
  return (
    row.lesson.title.toLowerCase().includes(query)
    || row.module.title.toLowerCase().includes(query)
  );
}

export function filterPreparationRows(
  rows: LessonPreparationRow[],
  filter: PreparationFilter,
  searchQuery: string,
): LessonPreparationRow[] {
  return rows.filter((row) => matchesPreparationFilter(row, filter, searchQuery));
}

export function getBulkActionEligibility(
  rows: LessonPreparationRow[],
  selectedIds: ReadonlySet<string>,
): BulkActionEligibility {
  const selectedRows = rows.filter((row) => selectedIds.has(row.lesson.id));
  return {
    selectedCount: selectedRows.length,
    preparableIds: selectedRows
      .filter((row) => row.status === 'not_started')
      .map((row) => row.lesson.id),
    pausableJobs: deduplicateJobs(
      selectedRows
        .filter((row) => row.status === 'running' || row.status === 'queued')
        .map((row) => row.job),
    ),
    resumableJobs: deduplicateJobs(
      selectedRows
        .filter((row) => row.status === 'paused')
        .map((row) => row.job),
    ),
    retryableJobs: deduplicateJobs(
      selectedRows
        .filter((row) => row.status === 'failed')
        .map((row) => row.job),
    ),
    resettableIds: selectedRows
      .filter((row) => row.status !== 'not_started')
      .map((row) => row.lesson.id),
  };
}
