import { useEffect, useMemo, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { CirclePause, LoaderCircle, Play, RotateCcw, Search, Trash2, X } from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  useCancelLearningGenerationJob,
  usePrepareLearningLessons,
  usePrioritizeLearningGenerationJob,
  useResetLearningLessonPreparation,
  useRetryLearningGenerationJob,
} from '@/features/learning/curriculum/useLearningPlan';
import {
  buildPreparationControlModel,
  filterPreparationRows,
  getBulkActionEligibility,
  type PreparationFilter,
  type PreparationJobWithRank,
} from '@/features/learning/lessons/preparationControlModel';
import {
  PreparationControlModuleSection,
  TriStateCheckbox,
} from '@/features/learning/lessons/PreparationControlModuleSection';
import { paceLine } from '@/features/learning/lessons/preparationQueue';
import type { LearningModuleDto, LearningProgramDto } from '@/lib/bindings';

export type LessonPreparationControlCenterProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  program: LearningProgramDto;
  planJobs: PreparationJobWithRank[];
  programRevision: number;
  waitsForIndex?: string | null;
  paceMinutes?: number | null;
  onOpenLesson: (module: LearningModuleDto, lessonId: string) => void;
  onReview?: (programId: string, lessonId: string) => void;
};

type PendingReset = {
  lessonIds: string[];
  label: string;
};

const FILTER_OPTIONS: Array<{ id: PreparationFilter; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'active', label: 'Active' },
  { id: 'paused', label: 'Paused' },
  { id: 'failed', label: 'Failed' },
  { id: 'needs_review', label: 'Needs review' },
  { id: 'not_started', label: 'Not started' },
  { id: 'ready', label: 'Ready' },
];

function extractErrorMessage(err: unknown): string {
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === 'string' && err) return err;
  return 'The action could not be completed.';
}

export function LessonPreparationControlCenter({
  open,
  onOpenChange,
  program,
  planJobs,
  programRevision,
  waitsForIndex = null,
  paceMinutes = null,
  onOpenLesson,
  onReview,
}: LessonPreparationControlCenterProps) {
  const prepareLessons = usePrepareLearningLessons();
  const cancelJob = useCancelLearningGenerationJob();
  const retryJob = useRetryLearningGenerationJob();
  const prioritizeJob = usePrioritizeLearningGenerationJob();
  const resetPreparation = useResetLearningLessonPreparation();

  const [filter, setFilter] = useState<PreparationFilter>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [collapsedModules, setCollapsedModules] = useState<Set<string>>(() => new Set());
  const [pendingReset, setPendingReset] = useState<PendingReset | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionStatus, setActionStatus] = useState<string | null>(null);
  const [busyOperation, setBusyOperation] = useState(false);
  const [now, setNow] = useState(() => Date.now());

  const model = useMemo(() => buildPreparationControlModel(program, planJobs), [program, planJobs]);
  const validLessonIds = useMemo(() => new Set(model.rows.map((row) => row.lesson.id)), [model.rows]);
  const activeSelectedIds = useMemo(() => {
    const next = new Set<string>();
    for (const id of selectedIds) {
      if (validLessonIds.has(id)) next.add(id);
    }
    return next;
  }, [selectedIds, validLessonIds]);

  const filteredRows = useMemo(
    () => filterPreparationRows(model.rows, filter, searchQuery),
    [model.rows, filter, searchQuery],
  );
  const visibleIds = useMemo(() => filteredRows.map((row) => row.lesson.id), [filteredRows]);

  const moduleSections = useMemo(() => {
    const visibleSet = new Set(visibleIds);
    return model.modules
      .map((group) => ({
        ...group,
        visibleRows: group.rows.filter((row) => visibleSet.has(row.lesson.id)),
      }))
      .filter((group) => group.visibleRows.length > 0);
  }, [model.modules, visibleIds]);

  const eligibility = useMemo(
    () => getBulkActionEligibility(model.rows, activeSelectedIds),
    [model.rows, activeSelectedIds],
  );

  const hasRunningJobs = model.counts.running > 0;
  useEffect(() => {
    if (!open || !hasRunningJobs) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [open, hasRunningJobs]);

  useEffect(() => {
    if (!open) {
      setPendingReset(null);
      setActionError(null);
      setActionStatus(null);
    }
  }, [open]);

  const isMutating =
    busyOperation
    || prepareLessons.isPending
    || cancelJob.isPending
    || retryJob.isPending
    || prioritizeJob.isPending
    || resetPreparation.isPending;

  const visibleSelectedCount = visibleIds.filter((id) => activeSelectedIds.has(id)).length;
  const allVisibleSelected = visibleIds.length > 0 && visibleSelectedCount === visibleIds.length;
  const someVisibleSelected = visibleSelectedCount > 0 && visibleSelectedCount < visibleIds.length;

  const toggleSelectAllVisible = () => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (allVisibleSelected) {
        for (const id of visibleIds) next.delete(id);
      } else {
        for (const id of visibleIds) next.add(id);
      }
      return next;
    });
  };

  const toggleModuleSelection = (lessonIds: string[]) => {
    const allSelected = lessonIds.length > 0 && lessonIds.every((id) => activeSelectedIds.has(id));
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (allSelected) {
        for (const id of lessonIds) next.delete(id);
      } else {
        for (const id of lessonIds) next.add(id);
      }
      return next;
    });
  };

  const toggleLessonSelection = (lessonId: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(lessonId)) next.delete(lessonId);
      else next.add(lessonId);
      return next;
    });
  };

  const toggleModuleCollapsed = (moduleId: string) => {
    setCollapsedModules((prev) => {
      const next = new Set(prev);
      if (next.has(moduleId)) next.delete(moduleId);
      else next.add(moduleId);
      return next;
    });
  };

  const runBatchJobAction = async (
    label: string,
    jobs: PreparationJobWithRank[],
    mutate: (request: { operationId: string; programId: string; jobId: string; expectedRevision: number }) => Promise<unknown>,
  ) => {
    if (jobs.length === 0 || isMutating) return;
    setBusyOperation(true);
    setActionError(null);
    setActionStatus(`${label}…`);
    try {
      const results = await Promise.allSettled(
        jobs.map((job) =>
          mutate({
            operationId: crypto.randomUUID(),
            programId: program.summary.id,
            jobId: job.id,
            expectedRevision: programRevision,
          }),
        ),
      );
      const failures = results.filter((res): res is PromiseRejectedResult => res.status === 'rejected');
      if (failures.length > 0) {
        const firstReason = extractErrorMessage(failures[0].reason);
        setActionError(
          failures.length === jobs.length
            ? firstReason
            : `${jobs.length - failures.length} of ${jobs.length} succeeded; ${failures.length} failed: ${firstReason}`,
        );
      }
    } finally {
      setActionStatus(null);
      setBusyOperation(false);
    }
  };

  const handlePrepareLessons = async (lessonIds: string[], label: string) => {
    if (lessonIds.length === 0 || isMutating) return;
    setBusyOperation(true);
    setActionError(null);
    setActionStatus(label);
    try {
      await prepareLessons.mutateAsync({
        operationId: crypto.randomUUID(),
        programId: program.summary.id,
        lessonIds,
        expectedRevision: programRevision,
      });
    } catch (err) {
      setActionError(extractErrorMessage(err));
    } finally {
      setActionStatus(null);
      setBusyOperation(false);
    }
  };

  const handlePrioritizeJob = async (job: PreparationJobWithRank) => {
    if (isMutating) return;
    setBusyOperation(true);
    setActionError(null);
    setActionStatus('Moving job to front of queue…');
    try {
      await prioritizeJob.mutateAsync({
        operationId: crypto.randomUUID(),
        programId: program.summary.id,
        jobId: job.id,
        expectedRevision: programRevision,
      });
    } catch (err) {
      setActionError(extractErrorMessage(err));
    } finally {
      setActionStatus(null);
      setBusyOperation(false);
    }
  };

  const handleConfirmReset = async () => {
    if (!pendingReset || isMutating) return;
    setBusyOperation(true);
    setActionError(null);
    setActionStatus(
      `Resetting ${pendingReset.lessonIds.length} ${pendingReset.lessonIds.length === 1 ? 'lesson' : 'lessons'}…`,
    );
    try {
      await resetPreparation.mutateAsync({
        operationId: crypto.randomUUID(),
        programId: program.summary.id,
        lessonIds: pendingReset.lessonIds,
        expectedRevision: programRevision,
      });
      setSelectedIds((prev) => {
        const next = new Set(prev);
        for (const id of pendingReset.lessonIds) next.delete(id);
        return next;
      });
      setPendingReset(null);
    } catch (err) {
      setActionError(extractErrorMessage(err));
    } finally {
      setActionStatus(null);
      setBusyOperation(false);
    }
  };

  const filterCountFor = (id: PreparationFilter): number => {
    switch (id) {
      case 'all':
        return model.counts.total;
      case 'active':
        return model.counts.active;
      case 'paused':
        return model.counts.paused;
      case 'failed':
        return model.counts.failed;
      case 'needs_review':
        return model.counts.needs_review;
      case 'not_started':
        return model.counts.not_started;
      case 'ready':
        return model.counts.ready;
    }
  };

  const readinessPercent = model.counts.total > 0
    ? Math.min(100, Math.round((model.counts.ready / model.counts.total) * 100))
    : 0;

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-[hsl(var(--overlay))] backdrop-blur-[1px]" />
        <Dialog.Content className="fixed inset-y-0 right-0 z-50 flex h-full w-full max-w-[46rem] flex-col border-l border-border bg-surface-overlay shadow-2xl outline-hidden">
          <header className="shrink-0 border-b border-border-subtle bg-surface px-5 py-4">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <Dialog.Title className="font-sans text-base font-semibold tracking-[-0.02em] text-text-primary">
                  Lesson Preparation Control Center
                </Dialog.Title>
                <Dialog.Description className="mt-0.5 text-xs text-text-secondary">
                  Queue, pause, resume, prioritize, or reset lesson preparation across {program.summary.title}.
                </Dialog.Description>
              </div>
              <Dialog.Close
                aria-label="Close"
                className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-text-secondary transition-colors hover:bg-surface-raised hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                <X size={16} aria-hidden="true" />
              </Dialog.Close>
            </div>

            <div className="mt-3 space-y-1.5">
              <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                <span className="font-medium tabular-nums text-text-primary">
                  {model.counts.ready} of {model.counts.total} ready
                </span>
                {paceMinutes != null && (
                  <span className="text-text-muted">{paceLine(paceMinutes)}</span>
                )}
              </div>
              <div
                role="progressbar"
                aria-label="Course preparation readiness"
                aria-valuenow={model.counts.ready}
                aria-valuemin={0}
                aria-valuemax={model.counts.total}
                className="h-1.5 overflow-hidden rounded-full bg-border-subtle"
              >
                <div
                  className="h-full rounded-full bg-accent transition-[width] motion-reduce:transition-none"
                  style={{ width: `${readinessPercent}%` }}
                />
              </div>
            </div>

            {(model.globalPausableJobs.length > 0
              || model.globalResumableJobs.length > 0
              || model.globalRetryableJobs.length > 0
              || model.globalNotStartedIds.length > 0) && (
              <div className="mt-3 flex flex-wrap items-center gap-2">
                {model.globalPausableJobs.length > 0 && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isMutating}
                    onClick={() =>
                      void runBatchJobAction(
                        'Pausing active jobs',
                        model.globalPausableJobs,
                        cancelJob.mutateAsync,
                      )
                    }
                  >
                    <CirclePause size={13} aria-hidden="true" />
                    Pause all
                  </Button>
                )}
                {model.globalResumableJobs.length > 0 && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isMutating || Boolean(waitsForIndex)}
                    onClick={() =>
                      void runBatchJobAction(
                        'Resuming paused jobs',
                        model.globalResumableJobs,
                        retryJob.mutateAsync,
                      )
                    }
                  >
                    <Play size={13} aria-hidden="true" />
                    Resume all
                  </Button>
                )}
                {model.globalRetryableJobs.length > 0 && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isMutating || Boolean(waitsForIndex)}
                    onClick={() =>
                      void runBatchJobAction(
                        'Retrying failed jobs',
                        model.globalRetryableJobs,
                        retryJob.mutateAsync,
                      )
                    }
                  >
                    <RotateCcw size={13} aria-hidden="true" />
                    Retry failed
                  </Button>
                )}
                {model.globalNotStartedIds.length > 0 && (
                  <Button
                    type="button"
                    size="sm"
                    disabled={isMutating || Boolean(waitsForIndex)}
                    onClick={() =>
                      void handlePrepareLessons(
                        model.globalNotStartedIds,
                        `Queueing ${model.globalNotStartedIds.length} remaining lessons…`,
                      )
                    }
                  >
                    Prepare remaining
                  </Button>
                )}
              </div>
            )}

            {waitsForIndex && (
              <p className="mt-2 text-xs text-text-secondary">{waitsForIndex}</p>
            )}
          </header>

          <div className="shrink-0 space-y-2.5 border-b border-border-subtle bg-bg/70 px-5 py-3">
            <div className="flex flex-wrap gap-1.5" role="group" aria-label="Filter lessons by status">
              {FILTER_OPTIONS.map((option) => {
                const count = filterCountFor(option.id);
                const selected = filter === option.id;
                return (
                  <button
                    key={option.id}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => setFilter(option.id)}
                    className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
                      selected
                        ? 'border-accent/40 bg-accent-muted text-accent'
                        : 'border-border-subtle bg-surface text-text-secondary hover:border-border-default hover:text-text-primary'
                    }`}
                  >
                    <span>{option.label}</span>
                    <span className="tabular-nums opacity-75">{count}</span>
                  </button>
                );
              })}
            </div>

            <div className="flex flex-wrap items-center gap-2.5">
              <div className="relative min-w-[12rem] flex-1">
                <Search
                  size={14}
                  aria-hidden="true"
                  className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted"
                />
                <input
                  type="search"
                  value={searchQuery}
                  onChange={(event) => setSearchQuery(event.target.value)}
                  placeholder="Filter lessons or modules…"
                  aria-label="Filter lessons or modules"
                  className="h-8 w-full rounded-lg border border-border-default bg-surface pl-8 pr-7 text-xs text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-hidden"
                />
                {searchQuery && (
                  <button
                    type="button"
                    aria-label="Clear search"
                    onClick={() => setSearchQuery('')}
                    className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-text-muted hover:text-text-primary"
                  >
                    <X size={12} aria-hidden="true" />
                  </button>
                )}
              </div>

              <label className="inline-flex cursor-pointer items-center gap-2 text-xs text-text-secondary">
                <TriStateCheckbox
                  checked={allVisibleSelected}
                  indeterminate={someVisibleSelected}
                  disabled={visibleIds.length === 0}
                  onChange={toggleSelectAllVisible}
                  ariaLabel="Select all visible lessons"
                />
                <span>Select all</span>
              </label>

              {activeSelectedIds.size > 0 && (
                <div className="flex items-center gap-2 text-xs">
                  <span className="font-medium tabular-nums text-text-primary">
                    {activeSelectedIds.size} selected
                  </span>
                  <button
                    type="button"
                    onClick={() => setSelectedIds(new Set())}
                    className="text-text-secondary underline hover:text-text-primary"
                  >
                    Clear
                  </button>
                </div>
              )}
            </div>
          </div>

          {eligibility.selectedCount > 0 && (
            <div
              role="region"
              aria-label="Bulk selection actions"
              className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-accent/25 bg-accent/5 px-5 py-2.5"
            >
              <span className="text-xs font-semibold tabular-nums text-text-primary">
                {eligibility.selectedCount} selected
              </span>
              <div className="flex flex-wrap items-center gap-1.5">
                {eligibility.preparableIds.length > 0 && (
                  <Button
                    type="button"
                    size="sm"
                    disabled={isMutating || Boolean(waitsForIndex)}
                    onClick={() =>
                      void handlePrepareLessons(
                        eligibility.preparableIds,
                        `Queueing ${eligibility.preparableIds.length} selected lessons…`,
                      )
                    }
                  >
                    Prepare ({eligibility.preparableIds.length})
                  </Button>
                )}
                {eligibility.pausableJobs.length > 0 && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isMutating}
                    onClick={() =>
                      void runBatchJobAction(
                        'Pausing selected jobs',
                        eligibility.pausableJobs,
                        cancelJob.mutateAsync,
                      )
                    }
                  >
                    Pause ({eligibility.pausableJobs.length})
                  </Button>
                )}
                {eligibility.resumableJobs.length > 0 && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isMutating || Boolean(waitsForIndex)}
                    onClick={() =>
                      void runBatchJobAction(
                        'Resuming selected jobs',
                        eligibility.resumableJobs,
                        retryJob.mutateAsync,
                      )
                    }
                  >
                    Resume ({eligibility.resumableJobs.length})
                  </Button>
                )}
                {eligibility.retryableJobs.length > 0 && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isMutating || Boolean(waitsForIndex)}
                    onClick={() =>
                      void runBatchJobAction(
                        'Retrying selected jobs',
                        eligibility.retryableJobs,
                        retryJob.mutateAsync,
                      )
                    }
                  >
                    Retry ({eligibility.retryableJobs.length})
                  </Button>
                )}
                {eligibility.resettableIds.length > 0 && (
                  <Button
                    type="button"
                    variant="destructive"
                    size="sm"
                    disabled={isMutating}
                    onClick={() =>
                      setPendingReset({
                        lessonIds: eligibility.resettableIds,
                        label: `${eligibility.resettableIds.length} selected ${eligibility.resettableIds.length === 1 ? 'lesson' : 'lessons'}`,
                      })
                    }
                  >
                    <Trash2 size={13} aria-hidden="true" />
                    Reset / Delete ({eligibility.resettableIds.length})
                  </Button>
                )}
              </div>
            </div>
          )}

          {pendingReset && (
            <div
              role="alertdialog"
              aria-label="Confirm preparation reset"
              className="shrink-0 border-b border-danger/30 bg-danger-muted px-5 py-3 text-xs text-danger-fg"
            >
              <p className="font-semibold">
                Reset preparation for {pendingReset.label}?
              </p>
              <p className="mt-1 leading-5">
                This will cancel any active or paused jobs and permanently wipe saved drafts, checkpoints, claim reviews, and prepared content for{' '}
                {pendingReset.lessonIds.length === 1 ? 'this lesson' : `these ${pendingReset.lessonIds.length} lessons`}.
              </p>
              <div className="mt-2.5 flex flex-wrap items-center gap-2">
                <Button
                  type="button"
                  variant="destructive"
                  size="sm"
                  disabled={isMutating}
                  onClick={() => void handleConfirmReset()}
                >
                  Confirm reset
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={isMutating}
                  onClick={() => setPendingReset(null)}
                >
                  Cancel
                </Button>
              </div>
            </div>
          )}

          {(actionStatus || actionError) && (
            <div className="shrink-0 border-b border-border-subtle px-5 py-2">
              {actionStatus && (
                <p role="status" className="flex items-center gap-2 text-xs text-text-secondary">
                  <LoaderCircle size={13} className="animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" />
                  <span>{actionStatus}</span>
                </p>
              )}
              {actionError && (
                <div role="alert" className="flex items-start justify-between gap-3 rounded-lg border border-danger/30 bg-danger-muted px-3 py-2 text-xs text-danger-fg">
                  <span>{actionError}</span>
                  <button
                    type="button"
                    onClick={() => setActionError(null)}
                    aria-label="Dismiss error"
                    className="shrink-0 underline"
                  >
                    Dismiss
                  </button>
                </div>
              )}
            </div>
          )}

          <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
            {moduleSections.length === 0 ? (
              <div className="rounded-xl border border-dashed border-border bg-bg p-8 text-center">
                <p className="text-sm font-medium text-text-primary">No matching lessons</p>
                <p className="mt-1 text-xs text-text-secondary">
                  Adjust the status filter or search query to see lessons in this course.
                </p>
              </div>
            ) : (
              <div className="space-y-4">
                {moduleSections.map((group) => (
                  <PreparationControlModuleSection
                    key={group.module.id}
                    group={group}
                    collapsed={collapsedModules.has(group.module.id)}
                    selectedIds={activeSelectedIds}
                    isMutating={isMutating}
                    waitsForIndex={waitsForIndex}
                    now={now}
                    onToggleCollapse={toggleModuleCollapsed}
                    onToggleModuleSelection={toggleModuleSelection}
                    onToggleLessonSelection={toggleLessonSelection}
                    onPrepareLessons={(lessonIds, label) => void handlePrepareLessons(lessonIds, label)}
                    onPauseJob={(job, title) =>
                      void runBatchJobAction(`Pausing ${title}`, [job], cancelJob.mutateAsync)
                    }
                    onResumeOrRetryJob={(job, label) =>
                      void runBatchJobAction(label, [job], retryJob.mutateAsync)
                    }
                    onPrioritizeJob={(job) => void handlePrioritizeJob(job)}
                    onRequestReset={(lessonId, label) => setPendingReset({ lessonIds: [lessonId], label })}
                    onOpenLesson={(mod, lessonId) => {
                      onOpenLesson(mod, lessonId);
                      onOpenChange(false);
                    }}
                    onReviewLesson={(lessonId, mod) => {
                      if (onReview) onReview(program.summary.id, lessonId);
                      else onOpenLesson(mod, lessonId);
                      onOpenChange(false);
                    }}
                  />
                ))}
              </div>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
