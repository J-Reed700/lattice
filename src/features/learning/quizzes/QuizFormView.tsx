import { LoaderCircle } from 'lucide-react';

import { QuizResult, type LessonCompletion } from '@/features/learning/quizzes/QuizResult';
import { QuizRunner } from '@/features/learning/quizzes/QuizRunner';
import { BackLink, quietPill } from '@/features/learning/quizzes/RunnerParts';
import { scopeKey, useQuizLauncher } from '@/features/learning/quizzes/useQuizLauncher';
import { allQuizStatuses, useQuizForm } from '@/features/learning/quizzes/useQuizzes';
import type { LearningProgramDto, LearningQuizOverviewDto } from '@/lib/bindings';

/** One quiz form: the runner while it is open, the result once submitted. */
export function QuizFormView({ formId, program, overview, onBack, onOpenForm, completionFor }: {
  formId: string; program: LearningProgramDto; overview?: LearningQuizOverviewDto; onBack: () => void; onOpenForm: (formId: string) => void;
  completionFor?: (lessonId: string) => LessonCompletion | undefined;
}) {
  const query = useQuizForm(formId);
  const launcher = useQuizLauncher(program.summary.id, onOpenForm);
  const form = query.data;
  if (query.isLoading) return <div role="status" className="flex items-center gap-2 rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted"><LoaderCircle size={16} className="animate-spin motion-reduce:animate-none" aria-hidden="true" /> Opening your quiz…</div>;
  if (query.isError || !form) return <div className="space-y-3"><BackLink label="Quizzes" onClick={onBack} />
    <section role="alert" className="rounded-2xl border border-border bg-surface p-6">
      <h3 className="font-serif text-xl text-text-primary">This quiz could not be opened</h3>
      <p className="mt-2 text-sm text-text-secondary">{query.error?.message ?? 'It may have been removed by an import.'}</p>
      <button type="button" onClick={() => void query.refetch()} className={`${quietPill} mt-4`}>Try again</button>
    </section></div>;
  const status = form.scope && overview ? allQuizStatuses(overview).find((entry) => scopeKey(entry.scope) === scopeKey(form.scope!)) : undefined;
  if (form.status === 'active') {
    return <QuizRunner form={form} sources={program.sources} coverage={status?.coverage} onExit={onBack}
      onSubmitted={(submitted) => onOpenForm(submitted.id)} reload={async () => (await query.refetch()).data} />;
  }
  if (form.status === 'interrupted') return <div className="space-y-3"><BackLink label="Quizzes" onClick={onBack} />
    <section className="rounded-2xl border border-border bg-surface p-6">
      <h3 className="font-serif text-xl text-text-primary">This attempt was paused and set aside</h3>
      <p className="mt-2 text-sm text-text-secondary">It was not graded and counts for nothing either way. Start a fresh attempt whenever you like.</p>
    </section></div>;
  const lessonId = form.scope?.kind === 'lesson' ? form.scope.id : null;
  return <QuizResult form={form} sources={program.sources} programRevision={program.summary.revision} onBack={onBack}
    onRetake={status ? (mode) => void launcher.retake(status, mode) : undefined} retakePending={status ? launcher.isBusy(status) : false}
    retakeError={status ? launcher.errorFor(status) : undefined} completion={lessonId ? completionFor?.(lessonId) : undefined} />;
}
