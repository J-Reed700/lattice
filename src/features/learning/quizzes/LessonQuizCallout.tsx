import { ClipboardCheck, LoaderCircle } from 'lucide-react';

import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { freshnessWords, quizStatusWords } from '@/features/learning/quizzes/quizCopy';
import { QuizStatusActions } from '@/features/learning/quizzes/QuizStatusRow';
import { VerdictChip } from '@/features/learning/quizzes/QuizVerdict';
import { useQuizLauncher } from '@/features/learning/quizzes/useQuizLauncher';
import { lessonQuizStatus, useQuizOverview } from '@/features/learning/quizzes/useQuizzes';
import type { LearningLessonDto, LearningProgramDto } from '@/lib/bindings';

/** "Preparing questions… step 2 of 3 (writing short answers)", from the bank job when there is one. */
function usePreparingLine(programId: string, jobId: string | null, fallback: string) {
  const plan = useLearningPlan(jobId ? programId : '');
  const job = plan.data?.jobs?.find((entry) => entry.id === jobId);
  if (!job || job.progressTotal <= 0) return fallback || 'Preparing questions…';
  const step = Math.min(job.progressCompleted + 1, job.progressTotal);
  return `Preparing questions… step ${step} of ${job.progressTotal}${job.progressMessage ? ` (${job.progressMessage})` : ''}`;
}

/** The lesson quiz at the end of a lesson: its state in words and one clear next action. */
export function LessonQuizCallout({ program, lesson, onOpenForm, onViewResult }: {
  program: LearningProgramDto; lesson: LearningLessonDto; onOpenForm: (formId: string) => void; onViewResult: (formId: string) => void;
}) {
  const active = program.summary.status === 'active';
  const overview = useQuizOverview(program.summary.id, active);
  const launcher = useQuizLauncher(program.summary.id, onOpenForm);
  const status = lessonQuizStatus(overview.data, lesson.id);
  const preparing = usePreparingLine(program.summary.id, status?.bankStatus === 'preparing' ? status.preparingJobId : null, status?.bankMessage ?? '');
  if (!active || overview.isError) return null;
  const words = status ? quizStatusWords(status) : null;
  const error = status ? launcher.errorFor(status) : undefined;
  return <section id={`quiz-callout-${lesson.id}`} tabIndex={-1} aria-labelledby={`quiz-callout-${lesson.id}-title`}
    className="mt-6 rounded-2xl border border-accent/25 bg-gradient-to-br from-accent/[.07] to-transparent p-5 outline-hidden focus-visible:ring-2 focus-visible:ring-accent sm:p-6">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-[.15em] text-accent"><ClipboardCheck size={14} aria-hidden="true" /> Lesson quiz</div>
      {status?.bankStatus === 'ready' && <span className="text-xs tabular-nums text-text-muted">{status.formSize} questions · about {status.expectedMinutes} min</span>}
    </div>
    <h4 id={`quiz-callout-${lesson.id}-title`} className="mt-1.5 font-serif text-xl leading-snug text-text-primary">Check what stuck from “{lesson.title}”</h4>
    <p className="mt-1.5 max-w-2xl text-sm leading-6 text-text-secondary">Quick choices, a matching or ordering task, a short answer and one written explanation. Keys are shown after you submit; nothing is locked.</p>
    {!status ? <p role="status" className="mt-4 flex items-center gap-2 text-sm text-text-muted"><LoaderCircle size={15} aria-hidden="true" className="animate-spin motion-reduce:animate-none" /> Checking your quiz…</p>
      : <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">{words && <VerdictChip words={words} />}
            {status.bankStatus === 'preparing' && <span role="status" className="text-xs text-text-secondary">{preparing}</span>}
            {status.bankStatus !== 'preparing' && words?.detail && <span className="text-xs text-text-muted">{words.detail}</span>}</div>
          {status.verdict === 'not_taken' && !status.activeFormId && status.bankStatus === 'ready' && <p className="mt-1 text-xs text-text-muted">Best: not taken yet</p>}
          {(status.verdict === 'passed' || status.verdict === 'not_passed') && <p className="mt-1 max-w-md text-xs leading-5 text-text-muted">{freshnessWords(status)}</p>}
          {error && <p role="alert" className="mt-1 text-xs text-rose-700 dark:text-rose-300">{error}</p>}
        </div>
        <div className="flex flex-wrap items-center gap-1.5"><QuizStatusActions status={status} launcher={launcher} onViewResult={onViewResult} retakeLabel="Retake with fresh questions" /></div>
      </div>}
  </section>;
}
