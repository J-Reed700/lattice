import { ArrowRight, BookOpenCheck, Clock } from 'lucide-react';

import type { DashboardCourseSummary } from '@/features/dashboard/hooks/useDashboardQuery';
import { cn } from '@/lib/utils';

export function buildCourseResumePath(course: Pick<DashboardCourseSummary, 'id' | 'currentLessonId'>): string {
  const params = new URLSearchParams({ course: course.id });
  if (course.currentLessonId) {
    params.set('lesson', course.currentLessonId);
  }
  return `/studio?${params.toString()}`;
}

function coursePercent(course: DashboardCourseSummary): number {
  if (course.lessonCount <= 0) return 0;
  return Math.min(100, Math.round((course.completedLessons / course.lessonCount) * 100));
}

function courseStatusInfo(course: DashboardCourseSummary): {
  label: string;
  cta: string;
  shortCta: string;
  pillClass: string;
} {
  if (course.status === 'draft') {
    return {
      label: 'Draft',
      cta: 'Review outline',
      shortCta: 'Outline',
      pillClass: 'bg-surface-raised text-text-tertiary',
    };
  }
  if (course.lessonCount > 0 && course.completedLessons >= course.lessonCount) {
    return {
      label: 'Completed',
      cta: 'Review course',
      shortCta: 'Review',
      pillClass: 'bg-success-muted text-success-fg',
    };
  }
  if (course.completedLessons === 0) {
    return {
      label: 'Ready',
      cta: 'Start lesson',
      shortCta: 'Start',
      pillClass: 'bg-accent-muted text-accent',
    };
  }
  return {
    label: 'In progress',
    cta: 'Continue lesson',
    shortCta: 'Resume',
    pillClass: 'bg-accent-muted text-accent',
  };
}

export interface ContinueLearningSectionProps {
  courses: readonly DashboardCourseSummary[];
  onNavigate: (path: string) => void;
}

export function ContinueLearningSection({ courses, onNavigate }: ContinueLearningSectionProps) {
  const primary = courses[0] ?? null;
  const secondary = courses.slice(1);

  return (
    <section>
      <div className="mb-2 flex items-baseline justify-between px-1">
        <h2 className="text-ui font-medium text-text-tertiary">Continue learning</h2>
        <button
          type="button"
          onClick={() => onNavigate('/studio')}
          className="text-xs text-text-muted transition-colors duration-fast hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          Open studio
        </button>
      </div>

      {!primary ? (
        <button
          type="button"
          onClick={() => onNavigate('/studio?new=1')}
          className="pressable group flex w-full items-center gap-3.5 rounded-xl border border-border-subtle bg-surface/60 px-4 py-3.5 text-left transition-[background-color,border-color,scale] duration-fast hover:border-border-default hover:bg-surface focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-accent-muted text-accent">
            <BookOpenCheck className="h-[17px] w-[17px]" strokeWidth={1.6} />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block text-ui font-medium text-text-primary">
              Start a course in Studio
            </span>
            <span className="mt-0.5 block truncate text-xs text-text-muted">
              Turn your documents and notes into structured lessons, practice, and recall.
            </span>
          </span>
          <span className="inline-flex shrink-0 items-center gap-1 text-xs font-medium text-accent">
            New course
            <ArrowRight className="h-3.5 w-3.5 transition-transform duration-fast group-hover:translate-x-0.5" strokeWidth={1.75} />
          </span>
        </button>
      ) : (
        <div className="space-y-2.5">
          {/* Primary course card */}
          {(() => {
            const hasLessons = primary.lessonCount > 0;
            const pct = coursePercent(primary);
            const status = courseStatusInfo(primary);
            return (
              <button
                type="button"
                onClick={() => onNavigate(buildCourseResumePath(primary))}
                className="pressable group block w-full rounded-xl border border-border-subtle bg-surface p-4 text-left shadow-xs transition-[border-color,box-shadow,scale] duration-base hover:border-border-default hover:shadow-sm focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                <div className="flex items-center justify-between gap-3">
                  <span
                    className={cn(
                      'inline-flex items-center rounded-full px-2 py-0.5 text-[11px] font-medium',
                      status.pillClass,
                    )}
                  >
                    {status.label}
                  </span>
                  {hasLessons ? (
                    <span className="text-xs tabular-nums text-text-muted">
                      {primary.completedLessons} of {primary.lessonCount}{' '}
                      {primary.lessonCount === 1 ? 'lesson' : 'lessons'} · {pct}%
                    </span>
                  ) : null}
                </div>

                <h3 className="mt-2.5 truncate font-serif text-[19px] font-medium leading-snug tracking-[-0.015em] text-text-primary">
                  {primary.title}
                </h3>

                {primary.currentLessonTitle ? (
                  <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs">
                    {primary.currentModuleTitle ? (
                      <span className="min-w-0 max-w-full truncate text-text-muted">
                        {primary.currentModuleTitle}
                      </span>
                    ) : null}
                    {primary.currentModuleTitle ? (
                      <span aria-hidden="true" className="shrink-0 text-text-disabled">·</span>
                    ) : null}
                    <span className="min-w-0 max-w-full truncate font-medium text-text-secondary">
                      {primary.currentLessonTitle}
                    </span>
                    {primary.currentLessonMinutes ? (
                      <span className="inline-flex shrink-0 items-center gap-1 tabular-nums text-text-muted">
                        <Clock className="h-3 w-3" strokeWidth={1.6} />
                        {primary.currentLessonMinutes}m
                      </span>
                    ) : null}
                  </div>
                ) : primary.goal ? (
                  <p className="mt-1 line-clamp-1 text-xs text-text-muted">{primary.goal}</p>
                ) : null}

                {hasLessons ? (
                  <div
                    role="progressbar"
                    aria-label={`${primary.title} progress`}
                    aria-valuenow={pct}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    className="mt-3.5 h-1.5 w-full overflow-hidden rounded-full bg-[hsl(var(--text-primary)/0.075)]"
                  >
                    <div
                      className="h-full rounded-full bg-accent transition-[width] duration-base"
                      style={{ width: `${Math.max(pct, primary.completedLessons > 0 ? 4 : 0)}%` }}
                    />
                  </div>
                ) : null}

                <div className="mt-3 flex items-center justify-between gap-2 text-xs">
                  <span className="tabular-nums text-text-muted">
                    {primary.moduleCount} {primary.moduleCount === 1 ? 'module' : 'modules'}
                  </span>
                  <span className="inline-flex items-center gap-1 font-medium text-accent">
                    {status.cta}
                    <ArrowRight
                      className="h-3.5 w-3.5 transition-transform duration-fast group-hover:translate-x-0.5"
                      strokeWidth={1.75}
                    />
                  </span>
                </div>
              </button>
            );
          })()}

          {/* Secondary active courses */}
          {secondary.length > 0 ? (
            <div className="grid gap-2.5 sm:grid-cols-2">
              {secondary.map((course) => {
                const hasLessons = course.lessonCount > 0;
                const pct = coursePercent(course);
                const status = courseStatusInfo(course);
                return (
                  <button
                    key={course.id}
                    type="button"
                    onClick={() => onNavigate(buildCourseResumePath(course))}
                    className="pressable group flex flex-col justify-between rounded-xl border border-border-subtle bg-surface/65 p-3.5 text-left transition-[background-color,border-color,scale] duration-fast hover:border-border-default hover:bg-surface focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <div>
                      <div className="flex items-center justify-between gap-2">
                        <span className="truncate text-ui font-medium text-text-primary">
                          {course.title}
                        </span>
                        {hasLessons ? (
                          <span className="shrink-0 text-[11px] tabular-nums text-text-muted">
                            {course.completedLessons}/{course.lessonCount}
                          </span>
                        ) : (
                          <span className="shrink-0 text-[11px] text-text-muted">
                            {status.label}
                          </span>
                        )}
                      </div>
                      <p className="mt-0.5 truncate text-xs text-text-muted">
                        {course.currentLessonTitle
                          ? course.currentLessonTitle
                          : course.goal || status.label}
                      </p>
                    </div>

                    <div className="mt-3 flex items-center gap-2.5">
                      {hasLessons ? (
                        <div
                          role="progressbar"
                          aria-label={`${course.title} progress`}
                          aria-valuenow={pct}
                          aria-valuemin={0}
                          aria-valuemax={100}
                          className="h-1 flex-1 overflow-hidden rounded-full bg-[hsl(var(--text-primary)/0.075)]"
                        >
                          <div
                            className="h-full rounded-full bg-accent"
                            style={{ width: `${Math.max(pct, course.completedLessons > 0 ? 6 : 0)}%` }}
                          />
                        </div>
                      ) : (
                        <span className="flex-1 text-[11px] text-text-muted">
                          {course.moduleCount > 0
                            ? `${course.moduleCount} ${course.moduleCount === 1 ? 'module' : 'modules'}`
                            : 'Outline draft'}
                        </span>
                      )}
                      <span className="shrink-0 text-[11px] font-medium text-accent opacity-80 transition-opacity duration-fast group-hover:opacity-100">
                        {status.shortCta} →
                      </span>
                    </div>
                  </button>
                );
              })}
            </div>
          ) : null}
        </div>
      )}
    </section>
  );
}
