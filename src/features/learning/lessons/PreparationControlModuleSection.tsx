import { useEffect, useRef } from 'react';

import {
  AlertCircle,
  ArrowUpToLine,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  CirclePause,
  ClipboardCheck,
  Clock3,
  LoaderCircle,
} from 'lucide-react';

import { Button } from '@/components/ui/button';
import { preparationDuration } from '@/features/learning/lessons/lessonPreparationActivity';
import type {
  LessonPreparationControlStatus,
  LessonPreparationRow,
  ModulePreparationGroup,
  PreparationJobWithRank,
} from '@/features/learning/lessons/preparationControlModel';
import type { LearningModuleDto } from '@/lib/bindings';
import { jobErrorToError, learnerError } from '@/lib/errorHandling';
import { ErrorCode } from '@/types/api/errorCodes';

const STATUS_BADGE_STYLES: Record<LessonPreparationControlStatus, string> = {
  running: 'bg-accent-muted text-accent border-accent/30',
  queued: 'bg-surface-raised text-text-secondary border-border-default',
  paused: 'bg-warning-muted text-warning-fg border-warning/30',
  failed: 'bg-danger-muted text-danger-fg border-danger/30',
  needs_review: 'bg-accent-muted text-accent border-accent/30',
  ready: 'bg-success-muted text-success-fg border-success/30',
  not_started: 'bg-bg text-text-muted border-border-subtle',
};

export function TriStateCheckbox({
  checked,
  indeterminate = false,
  onChange,
  ariaLabel,
  disabled = false,
}: {
  checked: boolean;
  indeterminate?: boolean;
  onChange: () => void;
  ariaLabel: string;
  disabled?: boolean;
}) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.indeterminate = indeterminate;
    }
  }, [indeterminate]);

  return (
    <input
      ref={inputRef}
      type="checkbox"
      checked={checked}
      disabled={disabled}
      onChange={onChange}
      aria-label={ariaLabel}
      className="h-4 w-4 shrink-0 cursor-pointer rounded border-border-default accent-[hsl(var(--accent))] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
    />
  );
}

function StatusPill({ row }: { row: LessonPreparationRow }) {
  const label = (() => {
    switch (row.status) {
      case 'running':
        return 'Running';
      case 'queued':
        return row.reconnecting
          ? 'Reconnecting'
          : row.queueIndex != null
            ? `Queued #${row.queueIndex}`
            : 'Queued';
      case 'paused':
        return 'Paused';
      case 'failed':
        return 'Failed';
      case 'needs_review':
        return 'Needs review';
      case 'ready':
        return 'Ready';
      case 'not_started':
        return 'Not started';
    }
  })();

  return (
    <span
      className={`inline-flex shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium ${STATUS_BADGE_STYLES[row.status]}`}
    >
      {row.status === 'running' && (
        <LoaderCircle size={11} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />
      )}
      {row.status === 'queued' && <Clock3 size={11} aria-hidden="true" />}
      {row.status === 'paused' && <CirclePause size={11} aria-hidden="true" />}
      {row.status === 'failed' && <AlertCircle size={11} aria-hidden="true" />}
      {row.status === 'needs_review' && <ClipboardCheck size={11} aria-hidden="true" />}
      {row.status === 'ready' && <CheckCircle2 size={11} aria-hidden="true" />}
      <span>{label}</span>
    </span>
  );
}

function conciseRowError(row: LessonPreparationRow): string {
  if (row.errorCode && row.errorCode !== ErrorCode.UNKNOWN && row.job) {
    return learnerError(jobErrorToError(row.job)).headline;
  }
  return row.errorMessage || 'Preparation stopped. Saved checkpoints are kept.';
}

export type PreparationControlModuleSectionProps = {
  group: ModulePreparationGroup & { visibleRows: LessonPreparationRow[] };
  collapsed: boolean;
  selectedIds: ReadonlySet<string>;
  isMutating: boolean;
  waitsForIndex: string | null;
  now: number;
  onToggleCollapse: (moduleId: string) => void;
  onToggleModuleSelection: (lessonIds: string[]) => void;
  onToggleLessonSelection: (lessonId: string) => void;
  onPrepareLessons: (lessonIds: string[], label: string) => void;
  onPauseJob: (job: PreparationJobWithRank, title: string) => void;
  onResumeOrRetryJob: (job: PreparationJobWithRank, label: string) => void;
  onPrioritizeJob: (job: PreparationJobWithRank) => void;
  onRequestReset: (lessonId: string, title: string) => void;
  onOpenLesson: (module: LearningModuleDto, lessonId: string) => void;
  onReviewLesson: (lessonId: string, module: LearningModuleDto) => void;
};

export function PreparationControlModuleSection({
  group,
  collapsed,
  selectedIds,
  isMutating,
  waitsForIndex,
  now,
  onToggleCollapse,
  onToggleModuleSelection,
  onToggleLessonSelection,
  onPrepareLessons,
  onPauseJob,
  onResumeOrRetryJob,
  onPrioritizeJob,
  onRequestReset,
  onOpenLesson,
  onReviewLesson,
}: PreparationControlModuleSectionProps) {
  const groupIds = group.visibleRows.map((row) => row.lesson.id);
  const selectedInGroup = groupIds.filter((id) => selectedIds.has(id)).length;
  const allGroupSelected = groupIds.length > 0 && selectedInGroup === groupIds.length;
  const someGroupSelected = selectedInGroup > 0 && selectedInGroup < groupIds.length;

  return (
    <section
      aria-label={`Module ${group.moduleIndex + 1}: ${group.module.title}`}
      className="overflow-hidden rounded-xl border border-border-subtle bg-surface"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border-subtle bg-surface-raised/60 px-3.5 py-2.5">
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          <TriStateCheckbox
            checked={allGroupSelected}
            indeterminate={someGroupSelected}
            onChange={() => onToggleModuleSelection(groupIds)}
            ariaLabel={`Select all lessons in ${group.module.title}`}
          />
          <button
            type="button"
            aria-expanded={!collapsed}
            onClick={() => onToggleCollapse(group.module.id)}
            className="flex min-w-0 flex-1 items-center gap-1.5 text-left focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            {collapsed ? (
              <ChevronRight size={14} className="shrink-0 text-text-muted" aria-hidden="true" />
            ) : (
              <ChevronDown size={14} className="shrink-0 text-text-muted" aria-hidden="true" />
            )}
            <span className="truncate text-xs font-semibold text-text-primary">
              Module {group.moduleIndex + 1} · {group.module.title}
            </span>
          </button>
        </div>

        <div className="flex items-center gap-2.5">
          <span className="text-xs tabular-nums text-text-muted">
            {group.readyCount} of {group.totalCount} ready
          </span>
          {group.notStartedIds.length > 0 && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={isMutating || Boolean(waitsForIndex)}
              onClick={() =>
                onPrepareLessons(
                  group.notStartedIds,
                  `Queueing ${group.notStartedIds.length} lessons in ${group.module.title}…`,
                )
              }
            >
              Prepare module
            </Button>
          )}
        </div>
      </div>

      {!collapsed && (
        <ul className="divide-y divide-border-subtle">
          {group.visibleRows.map((row) => {
            const checked = selectedIds.has(row.lesson.id);
            const elapsed =
              row.status === 'running' && row.startedAt != null
                ? Math.max(0, (now - row.startedAt) / 1000)
                : null;

            return (
              <li
                key={row.lesson.id}
                className="flex flex-col gap-2 px-3.5 py-3 transition-colors hover:bg-bg/60 sm:flex-row sm:items-start sm:justify-between"
              >
                <div className="flex min-w-0 flex-1 items-start gap-2.5">
                  <div className="pt-0.5">
                    <TriStateCheckbox
                      checked={checked}
                      onChange={() => onToggleLessonSelection(row.lesson.id)}
                      ariaLabel={`Select ${row.lesson.title}`}
                    />
                  </div>

                  <div className="min-w-0 flex-1 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="shrink-0 text-xs font-medium tabular-nums text-text-muted">
                        {row.ordinal}
                      </span>
                      <button
                        type="button"
                        onClick={() => onOpenLesson(row.module, row.lesson.id)}
                        className="min-w-0 truncate text-left text-sm font-medium text-text-primary hover:text-accent hover:underline focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                      >
                        {row.lesson.title}
                      </button>
                      <StatusPill row={row} />
                    </div>

                    <div className="space-y-1 text-xs text-text-secondary">
                      {(row.status === 'running' || row.status === 'queued' || row.status === 'paused') && (
                        <>
                          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                            {row.phaseLabel && (
                              <span className="font-medium text-text-primary">{row.phaseLabel}</span>
                            )}
                            {row.stageText && row.stageText !== row.phaseLabel && (
                              <span className="text-text-secondary">{row.stageText}</span>
                            )}
                            {elapsed != null && (
                              <span className="tabular-nums text-text-muted">
                                · {preparationDuration(elapsed)}
                              </span>
                            )}
                            {row.savedWorkHint && (
                              <span className="text-text-muted">· {row.savedWorkHint}</span>
                            )}
                          </div>
                          {row.checksTotal > 0 && (
                            <div className="flex items-center gap-2">
                              <div
                                role="progressbar"
                                aria-label={`${row.lesson.title} statement checks`}
                                aria-valuenow={row.checksCompleted}
                                aria-valuemin={0}
                                aria-valuemax={row.checksTotal}
                                className="h-1 flex-1 overflow-hidden rounded-full bg-border-subtle"
                              >
                                <div
                                  className="h-full rounded-full bg-accent transition-[width] motion-reduce:transition-none"
                                  style={{
                                    width: `${Math.min(100, (row.checksCompleted / row.checksTotal) * 100)}%`,
                                  }}
                                />
                              </div>
                              <span className="shrink-0 tabular-nums text-[11px] text-text-muted">
                                {row.checksCompleted}/{row.checksTotal}
                              </span>
                            </div>
                          )}
                        </>
                      )}

                      {row.status === 'failed' && (
                        <p className="text-danger-fg" title={row.errorMessage ?? undefined}>
                          {conciseRowError(row)}
                          {row.savedWorkHint ? ` · ${row.savedWorkHint}` : ''}
                        </p>
                      )}

                      {row.status === 'needs_review' && (
                        <p className="text-text-secondary">
                          Waiting for your statement review before publishing.
                        </p>
                      )}

                      {row.status === 'ready' && (
                        <p className="text-text-muted">
                          {row.lesson.completed ? 'Prepared and completed' : 'Prepared and ready to study'}
                        </p>
                      )}

                      {row.status === 'not_started' && (
                        <p className="text-text-muted">Outline only · not prepared yet</p>
                      )}
                    </div>
                  </div>
                </div>

                <div className="flex shrink-0 flex-wrap items-center gap-1.5 pl-6 sm:pl-0">
                  {row.status === 'not_started' && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={isMutating || Boolean(waitsForIndex)}
                      aria-label={`Prepare ${row.lesson.title}`}
                      onClick={() =>
                        onPrepareLessons([row.lesson.id], `Queueing ${row.lesson.title}…`)
                      }
                    >
                      Prepare
                    </Button>
                  )}

                  {row.status === 'running' && row.job && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={isMutating}
                      aria-label={`Pause ${row.lesson.title}`}
                      onClick={() => onPauseJob(row.job!, row.lesson.title)}
                    >
                      Pause
                    </Button>
                  )}

                  {row.status === 'queued' && row.job && (
                    <>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={isMutating}
                        aria-label={`Run next ${row.lesson.title}`}
                        onClick={() => onPrioritizeJob(row.job!)}
                      >
                        <ArrowUpToLine size={12} aria-hidden="true" />
                        Run next
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={isMutating}
                        aria-label={`Pause ${row.lesson.title}`}
                        onClick={() => onPauseJob(row.job!, row.lesson.title)}
                      >
                        Pause
                      </Button>
                    </>
                  )}

                  {row.status === 'paused' && row.job && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={isMutating || Boolean(waitsForIndex)}
                      aria-label={`Resume ${row.lesson.title}`}
                      onClick={() =>
                        onResumeOrRetryJob(row.job!, `Resuming ${row.lesson.title}`)
                      }
                    >
                      Resume
                    </Button>
                  )}

                  {row.status === 'failed' && row.job && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={isMutating || Boolean(waitsForIndex)}
                      aria-label={`Retry ${row.lesson.title}`}
                      onClick={() =>
                        onResumeOrRetryJob(row.job!, `Retrying ${row.lesson.title}`)
                      }
                    >
                      Retry
                    </Button>
                  )}

                  {row.status === 'needs_review' && (
                    <Button
                      type="button"
                      size="sm"
                      disabled={isMutating}
                      aria-label={`Review ${row.lesson.title}`}
                      onClick={() => onReviewLesson(row.lesson.id, row.module)}
                    >
                      Review
                    </Button>
                  )}

                  {row.status === 'ready' && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={isMutating}
                      aria-label={`Open ${row.lesson.title}`}
                      onClick={() => onOpenLesson(row.module, row.lesson.id)}
                    >
                      Open
                    </Button>
                  )}

                  {row.status !== 'not_started' && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      disabled={isMutating}
                      aria-label={`Reset ${row.lesson.title}`}
                      onClick={() => onRequestReset(row.lesson.id, row.lesson.title)}
                      className="text-danger-fg hover:bg-danger-muted hover:text-danger-fg"
                    >
                      Reset
                    </Button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
