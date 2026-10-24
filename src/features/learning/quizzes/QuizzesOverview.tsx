import { useEffect, useState } from 'react';

import { GraduationCap, LoaderCircle, Settings2, Sparkles } from 'lucide-react';

import { EvidenceTimeline } from '@/features/learning/quizzes/evidence/EvidenceTimeline';
import { FollowUps } from '@/features/learning/quizzes/evidence/FollowUps';
import { percent, scopeHeadline } from '@/features/learning/quizzes/quizCopy';
import { QuizFormView } from '@/features/learning/quizzes/QuizFormView';
import type { LessonCompletion } from '@/features/learning/quizzes/QuizResult';
import { QuizSettingsCard } from '@/features/learning/quizzes/QuizSettingsCard';
import { QuizStatusRow } from '@/features/learning/quizzes/QuizStatusRow';
import { eyebrow, parchmentBand, quietPill } from '@/features/learning/quizzes/RunnerParts';
import { useQuizLauncher, type QuizLauncher } from '@/features/learning/quizzes/useQuizLauncher';
import { allQuizStatuses, useQuizOverview } from '@/features/learning/quizzes/useQuizzes';
import type { LearningModuleQuizzesDto, LearningProgramDto, LearningQuizOverviewDto } from '@/lib/bindings';

const card = 'rounded-2xl border border-border bg-surface px-5 shadow-sm sm:px-6';
const headerButton = quietPill.replace('px-4', 'px-3');

function OverviewHeader({ overview, onSettings, onPrepareAll, preparable }: { overview: LearningQuizOverviewDto; onSettings: () => void; onPrepareAll: () => void; preparable: number }) {
  const { settings, quizzesPassed, quizzesTotal } = overview;
  return <header className={`${parchmentBand} overflow-hidden rounded-2xl border px-5 py-5 sm:px-7`}>
    <div className="grid items-start gap-5 md:grid-cols-[minmax(0,1fr)_auto]">
      <div className="min-w-0 max-w-2xl">
        <div className={`${eyebrow} flex items-center gap-1.5`}><GraduationCap size={14} aria-hidden="true" /> Quizzes &amp; tests</div>
        <h2 className="mt-1 font-serif text-[1.75rem] leading-tight text-text-primary">Check what stuck, whenever you like</h2>
        <p className="mt-1.5 text-sm leading-6 text-text-secondary">Every lesson has a quiz, every module a test, and the course an exam. Nothing is locked: take any of them in any order.</p>
        <p className="mt-1 text-xs text-text-muted">Pass marks: quiz {percent(settings.quizPass)} · test {percent(settings.testPass)} · exam {percent(settings.examPass)}</p>
      </div>
      <div className="w-full md:w-64">
        <div className="flex items-baseline justify-between text-xs"><span className="text-text-secondary">Lesson quizzes passed</span>
          <span className="font-semibold tabular-nums text-text-primary">{quizzesPassed}<span className="font-normal text-text-muted"> / {quizzesTotal}</span></span></div>
        <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-black/10 dark:bg-white/10"><div className="h-full rounded-full bg-accent transition-all" style={{ width: `${quizzesTotal ? (quizzesPassed / quizzesTotal) * 100 : 0}%` }} /></div>
        <div className="mt-3 flex flex-wrap gap-1.5 md:flex-nowrap md:justify-end">
          <button type="button" onClick={onSettings} className={headerButton}><Settings2 size={14} aria-hidden="true" /> Settings</button>
          {preparable > 0 && <button type="button" onClick={onPrepareAll} className={headerButton}><Sparkles size={14} aria-hidden="true" /> Prepare all</button>}
        </div>
      </div>
    </div>
  </header>;
}

function ModuleSection({ module, index, program, launcher, onOpenLesson, onViewResult }: {
  module: LearningModuleQuizzesDto; index: number; program: LearningProgramDto; launcher: QuizLauncher; onOpenLesson: (lessonId: string) => void; onViewResult: (formId: string) => void;
}) {
  const source = program.modules.find((entry) => entry.id === module.moduleId);
  return <section id={`quiz-section-${module.moduleId}`} tabIndex={-1} aria-labelledby={`quiz-section-${module.moduleId}-title`} className={`${card} outline-hidden focus-visible:ring-2 focus-visible:ring-accent`}>
    <h3 id={`quiz-section-${module.moduleId}-title`} className="pt-5 text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">
      Module {index + 1} · <span className="normal-case tracking-normal text-sm font-medium text-text-primary">{source?.title ?? module.test.title}</span>
    </h3>
    <div className="border-b border-border"><QuizStatusRow status={{ ...module.test, title: 'Module test' }} launcher={launcher} emphasis onViewResult={onViewResult} /></div>
    <ol aria-label={`Lesson quizzes in module ${index + 1}`} className="divide-y divide-border">{module.lessons.map((lesson, lessonIndex) => {
      const prepared = source?.lessons.find((entry) => entry.id === lesson.lessonId)?.preparation === 'ready';
      return <li key={lesson.lessonId}><QuizStatusRow status={lesson.quiz} launcher={launcher} number={`${lessonIndex + 1}.`} lessonPrepared={prepared}
        onOpenLesson={() => onOpenLesson(lesson.lessonId)} onViewResult={onViewResult} /></li>;
    })}</ol>
  </section>;
}

/**
 * The Quizzes tab: the course exam, then each module's test and lesson quizzes
 * with verdicts in words, beside the outcome evidence and follow-ups. A chosen
 * quiz replaces the list with its runner or result.
 */
export function QuizzesOverview({ program, formId, onFormIdChange, focusModuleId, onOpenLesson, completionFor }: {
  program: LearningProgramDto; formId: string | null; onFormIdChange: (formId: string | null) => void; focusModuleId?: string | null;
  onOpenLesson: (lessonId: string) => void; completionFor?: (lessonId: string) => LessonCompletion | undefined;
}) {
  const overview = useQuizOverview(program.summary.id);
  const launcher = useQuizLauncher(program.summary.id, onFormIdChange);
  const [settingsOpen, setSettingsOpen] = useState(false);
  useEffect(() => {
    if (!focusModuleId || formId || !overview.data) return;
    const target = document.getElementById(`quiz-section-${focusModuleId}`);
    target?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    target?.focus({ preventScroll: true });
  }, [focusModuleId, formId, overview.data]);

  if (formId) return <QuizFormView formId={formId} program={program} overview={overview.data} onBack={() => onFormIdChange(null)} onOpenForm={onFormIdChange} completionFor={completionFor} />;
  if (overview.isLoading) return <div role="status" className="flex items-center gap-2 rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted"><LoaderCircle size={16} className="animate-spin motion-reduce:animate-none" aria-hidden="true" /> Loading your quizzes…</div>;
  if (overview.isError || !overview.data) return <section role="alert" className="rounded-2xl border border-border bg-surface p-6">
    <h3 className="font-serif text-xl text-text-primary">Quizzes could not be loaded</h3>
    <p className="mt-2 text-sm text-text-secondary">{overview.error?.message ?? 'Try again in a moment.'}</p>
    <button type="button" onClick={() => void overview.refetch()} className={`${quietPill} mt-4`}>Try again</button>
  </section>;

  const data = overview.data;
  const readyLessons = new Set(program.modules.flatMap((module) => module.lessons.filter((lesson) => lesson.preparation === 'ready').map((lesson) => lesson.id)));
  const preparable = allQuizStatuses(data).filter((status) => (status.bankStatus === 'not_prepared' || status.bankStatus === 'failed') && (status.scope.kind !== 'lesson' || readyLessons.has(status.scope.id ?? '')));
  const prepareAll = async () => { for (const status of preparable) await launcher.prepareBank(status); };

  return <div className="space-y-5" data-testid="quizzes-overview">
    <OverviewHeader overview={data} preparable={preparable.length} onSettings={() => setSettingsOpen((open) => !open)} onPrepareAll={() => void prepareAll()} />
    {settingsOpen && <QuizSettingsCard programId={program.summary.id} settings={data.settings} onClose={() => setSettingsOpen(false)} />}
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(280px,340px)]">
      <div className="min-w-0 space-y-4">
        <section aria-labelledby="course-exam-title" className={card}>
          <h3 id="course-exam-title" className="pt-5 text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Course exam</h3>
          <QuizStatusRow status={{ ...data.exam, title: scopeHeadline.course }} launcher={launcher} emphasis onViewResult={onFormIdChange} />
        </section>
        {data.modules.map((module, index) => <ModuleSection key={module.moduleId} module={module} index={index} program={program} launcher={launcher} onOpenLesson={onOpenLesson} onViewResult={onFormIdChange} />)}
        {data.modules.length === 0 && <p className="rounded-2xl border border-dashed border-border bg-surface p-6 text-sm text-text-muted">This course has no lessons yet, so there is nothing to quiz.</p>}
      </div>
      <aside aria-label="Evidence and next steps" className="space-y-4">
        <EvidenceTimeline outcomes={data.outcomes} evidence={data.evidence} />
        <FollowUps programId={program.summary.id} outcomes={data.outcomes} followUps={data.followUps} />
      </aside>
    </div>
  </div>;
}
