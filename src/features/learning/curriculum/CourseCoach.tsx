import { ArrowRight, Info } from 'lucide-react';


import { nextCourseStep, type CourseStep } from '@/features/learning/curriculum/courseProgress';
import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { useLearningPracticeWorkspace } from '@/features/learning/practice/useLearningPractice';
import { useQuizOverview } from '@/features/learning/quizzes/useQuizzes';
import type { LearningProgramDto } from '@/lib/bindings';

export function CourseCoach({ program, dueCount, disabled, onStep }: { program: LearningProgramDto; dueCount: number; disabled: boolean; onStep: (step: CourseStep) => void }) {
  const practice = useLearningPracticeWorkspace(program.summary.id);
  const assessment = useQuizOverview(program.summary.id);
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
  return <section aria-label="Your next learning step" className="relative flex min-w-0 items-center gap-1">
    <button type="button" title={step.reason} disabled={disabled || learning} onClick={() => onStep(step)} className="pressable inline-flex h-8 min-w-0 max-w-[240px] items-center gap-1.5 rounded-lg bg-action px-3 text-xs font-medium text-action-fg shadow-action transition-[background-color,scale] duration-fast hover:bg-action-hover focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"><span className="truncate">{learning ? 'Finding your place…' : step.title}</span><ArrowRight size={13} className="shrink-0" /></button>
    <details className="relative shrink-0" onKeyDown={(event) => { if (event.key === 'Escape') { event.currentTarget.open = false; event.currentTarget.querySelector('summary')?.focus(); } }}><summary aria-label="Why this next step?" className="grid h-8 w-8 cursor-pointer list-none place-items-center rounded-lg text-text-tertiary transition-colors hover:bg-surface-raised hover:text-text-primary"><Info size={15} /></summary><div className="surface-pop absolute right-0 top-full z-30 mt-1.5 w-[min(22rem,calc(100vw-3rem))] rounded-xl bg-surface-overlay p-4 shadow-lg"><h3 className="text-sm font-semibold text-text-primary">Your next step</h3><p className="mt-1.5 text-xs leading-5 text-text-secondary">{step.reason}</p><p className="mt-2.5 text-xs leading-5 text-text-muted">{assessment.isError ? 'Application evidence is currently unavailable.' : `${observed} ${observed === 1 ? 'outcome has' : 'outcomes have'} application evidence. Lesson completion is tracked separately.`}</p>{!plan.data?.latestDiagnostic && <button type="button" disabled={disabled} onClick={() => onStep({ kind: 'starting_point', title: 'Check your starting point', reason: '' })} className="mt-2 text-xs font-medium text-accent underline underline-offset-4">Check your starting point</button>}{unavailable && <button type="button" onClick={() => { void practice.refetch(); void assessment.refetch(); void plan.refetch(); }} className="mt-2 text-xs font-medium text-accent underline">Retry unavailable progress</button>}</div></details>
  </section>;
}
