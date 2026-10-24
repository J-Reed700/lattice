import { ArrowRight, Info } from 'lucide-react';


import { useLearningAssessmentWorkspace } from '@/features/learning/assessment/useLearningAssessment';
import { nextCourseStep, type CourseStep } from '@/features/learning/curriculum/courseProgress';
import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { useLearningPracticeWorkspace } from '@/features/learning/practice/useLearningPractice';
import type { LearningProgramDto } from '@/lib/bindings';

export function CourseCoach({ program, dueCount, disabled, onStep }: { program: LearningProgramDto; dueCount: number; disabled: boolean; onStep: (step: CourseStep) => void }) {
  const practice = useLearningPracticeWorkspace(program.summary.id);
  const assessment = useLearningAssessmentWorkspace(program.summary.id);
  const plan = useLearningPlan(program.summary.id);
  const step = nextCourseStep(program, practice.data, assessment.data, plan.data, dueCount);
  const learning = practice.isLoading || assessment.isLoading || plan.isLoading;
  const unavailable = practice.isError || assessment.isError || plan.isError;
  const latest = new Map<string, NonNullable<typeof assessment.data>['evidence'][number]>();
  for (const event of assessment.data?.evidence ?? []) {
    if (!event.outcomeId || !['application', 'transfer'].includes(event.dimension)) continue;
    const key = `${event.outcomeId}:${event.dimension}`;
    if (!latest.has(key) || latest.get(key)!.observedAt < event.observedAt) latest.set(key, event);
  }
  const observed = new Set([...latest.values()].filter((e) => e.result === 'observed').map((e) => e.outcomeId)).size;
  return (
    <section aria-label="Your next learning step" className="relative flex min-w-0 items-center gap-1.5">
      <button
        type="button"
        title={step.reason}
        disabled={disabled || learning}
        onClick={() => onStep(step)}
        className="inline-flex min-h-9 min-w-0 max-w-full items-center gap-2 rounded-lg border border-accent/30 bg-accent-muted/40 px-3.5 py-2 text-xs font-medium text-text-primary hover:bg-accent-muted/70 transition-colors disabled:opacity-50 shadow-2xs"
      >
        <span className="truncate">{learning ? 'Finding your place…' : step.title}</span>
        <ArrowRight size={14} className="shrink-0 text-accent" />
      </button>
      <details
        className="relative shrink-0"
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.currentTarget.open = false;
            event.currentTarget.querySelector('summary')?.focus();
          }
        }}
      >
        <summary
          aria-label="Why this next step?"
          title="Why this next learning step"
          className="grid h-9 w-9 cursor-pointer list-none place-items-center rounded-lg border border-border-default bg-surface text-text-secondary hover:text-text-primary hover:bg-surface-raised transition-colors shadow-2xs"
        >
          <Info size={16} />
        </summary>
        <div className="absolute right-0 top-full z-30 mt-2 w-[min(22rem,calc(100vw-3rem))] rounded-xl border border-border-default bg-surface p-4 shadow-xl">
          <h3 className="text-sm font-semibold text-text-primary">Your next step</h3>
          <p className="mt-2 text-xs leading-5 text-text-secondary">{step.reason}</p>
          <p className="mt-3 text-xs leading-5 text-text-muted">
            {assessment.isError
              ? 'Application evidence is currently unavailable.'
              : `${observed} ${observed === 1 ? 'outcome has' : 'outcomes have'} application evidence. Lesson completion is tracked separately.`}
          </p>
          {!plan.data?.latestDiagnostic && (
            <button
              type="button"
              disabled={disabled}
              onClick={() => onStep({ kind: 'starting_point', title: 'Check your starting point', reason: '' })}
              className="mt-2 min-h-8 text-xs font-medium text-accent underline underline-offset-4 hover:brightness-110"
            >
              Check your starting point
            </button>
          )}
          {unavailable && (
            <button
              type="button"
              onClick={() => {
                void practice.refetch();
                void assessment.refetch();
                void plan.refetch();
              }}
              className="mt-2 min-h-8 text-xs font-medium text-accent underline hover:brightness-110"
            >
              Retry unavailable progress
            </button>
          )}
        </div>
      </details>
    </section>
  );
}
