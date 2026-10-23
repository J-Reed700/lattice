import { useEffect, useRef, useState } from 'react';

import { BookmarkPlus, CheckCircle2, LoaderCircle, RotateCcw, Square } from 'lucide-react';

import { formScopeKind, passMarkWords, percent, plural, scopeKindLabel, shortDate, shownScore, submissionHeadline, type GradingState } from '@/features/learning/quizzes/quizCopy';
import { QuizItemReview, type ItemActions } from '@/features/learning/quizzes/QuizItemReview';
import { VerdictIcon } from '@/features/learning/quizzes/QuizVerdict';
import { BackLink, eyebrow, parchmentBand, primaryPill, quietPill } from '@/features/learning/quizzes/RunnerParts';
import { ScoreRing } from '@/features/learning/quizzes/ScoreRing';
import {
  useAddQuizMissesToRecall, useContestQuizGrade, useOperationIds, useOverrideQuizGrade, useReportQuizItem, useResumeQuizGrading, useStopQuizGrading,
} from '@/features/learning/quizzes/useQuizzes';
import type { LearningAssessmentFormDto, LearningSourceDto } from '@/lib/bindings';

export type LessonCompletion = { completed: boolean; pending: boolean; onComplete: () => void };

const errorText = (error: unknown) => (error instanceof Error ? error.message : undefined);

function GradingBanner({ form, grading, onStop, onResume, stopping, resuming, error }: {
  form: LearningAssessmentFormDto; grading: GradingState; onStop: () => void; onResume: () => void; stopping: boolean; resuming: boolean; error?: string;
}) {
  const waiting = form.submission?.pendingItems ?? 0;
  if (!waiting) return null;
  const answers = plural(waiting, 'written answer');
  return <div role="status" aria-live="polite" className="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-accent/5 px-5 py-3 text-sm sm:px-8">
    {grading.active
      ? <p className="flex items-center gap-2 text-text-primary"><LoaderCircle size={16} aria-hidden="true" className="animate-spin text-accent motion-reduce:animate-none" /> Grading… {answers}. Your model is reading {waiting === 1 ? 'it' : 'them'}; the verdict waits for every grade.</p>
      : <p className="text-text-primary">{grading.stopped ? `Grading stopped · ${answers} waiting.` : `Couldn't grade yet · ${answers} waiting.`} Nothing is marked wrong while it waits.</p>}
    {grading.active
      ? <button type="button" disabled={stopping} onClick={onStop} className={quietPill}><Square size={13} aria-hidden="true" /> {stopping ? 'Stopping…' : 'Stop'}</button>
      : <button type="button" disabled={resuming} onClick={onResume} className={primaryPill}><RotateCcw size={14} aria-hidden="true" /> {resuming ? 'Starting…' : grading.stopped ? 'Resume grading' : 'Retry grading'}</button>}
    {error && <p role="alert" className="w-full text-rose-700 dark:text-rose-300">{error}</p>}
  </div>;
}

/**
 * The result: a verdict in words, the score with its pass mark, what graded
 * it, then every question with its key and reasons. Rewarding, never inflated.
 */
export function QuizResult({ form, sources, programRevision, onBack, onRetake, retakePending, retakeError, completion }: {
  form: LearningAssessmentFormDto; sources: LearningSourceDto[]; programRevision: number; onBack: () => void;
  onRetake?: (mode: 'graded' | 'practice') => void; retakePending: boolean; retakeError?: string; completion?: LessonCompletion;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  const operations = useOperationIds();
  const [stopped, setStopped] = useState(false);
  const [recalled, setRecalled] = useState<Set<string>>(new Set());
  const [reported, setReported] = useState<Set<string>>(new Set());
  const [recallNote, setRecallNote] = useState<string | null>(null);
  const resume = useResumeQuizGrading();
  const stop = useStopQuizGrading();
  const misses = useAddQuizMissesToRecall();
  const contest = useContestQuizGrade();
  const override = useOverrideQuizGrade();
  const report = useReportQuizItem();
  useEffect(() => { heading.current?.focus(); }, [form.id]);

  const submission = form.submission;
  const kind = formScopeKind(form);
  const practice = form.mode === 'practice';
  const headline = submissionHeadline(form);
  const shown = shownScore(form);
  const grading: GradingState = { active: Boolean(submission?.gradingJobId && submission.pendingItems > 0), stopped: stopped && !submission?.gradingJobId };
  const missedIds = (submission?.itemResults ?? []).filter((result) => result.pointsEarned < result.pointsPossible && result.gradeStatus !== 'pending' && !result.learnerOverride).map((result) => result.itemId).filter((id) => !recalled.has(id));
  const overridden = (submission?.itemResults ?? []).filter((result) => result.learnerOverride).length;
  const base = { programId: form.programId, formId: form.id };

  const addToRecall = (itemIds: string[]) => {
    const key = `recall:${itemIds.join(',')}`;
    misses.mutate({ ...base, operationId: operations.take(key), itemIds }, {
      onSuccess: (result) => { operations.done(key); setRecalled((current) => new Set([...current, ...itemIds])); setRecallNote(`Added ${plural(result.cardsCreated, 'card')} to Recall. You can edit them there.`); },
    });
  };
  const formAction = async (key: string, run: (operationId: string) => Promise<unknown>) => {
    try { await run(operations.take(key)); operations.done(key); return true; } catch { return false; }
  };
  const actions: ItemActions = {
    addToRecall: (itemId) => addToRecall([itemId]),
    recallAdded: (itemId) => recalled.has(itemId),
    recallPending: misses.isPending,
    contest: (itemId, note) => formAction(`contest:${itemId}`, (operationId) => contest.mutateAsync({ ...base, operationId, itemId, note })),
    contestPending: contest.isPending,
    contestError: errorText(contest.error),
    override: (itemId) => void formAction(`override:${itemId}`, (operationId) => override.mutateAsync({ ...base, operationId, itemId })),
    overridePending: override.isPending,
    overrideError: errorText(override.error),
    report: (itemId, note) => formAction(`report:${itemId}`, (operationId) => report.mutateAsync({ ...base, operationId, itemId, note })).then((ok) => { if (ok) setReported((current) => new Set([...current, itemId])); return ok; }),
    reportPending: report.isPending,
    reported: (itemId) => reported.has(itemId),
  };
  const stopGrading = () => {
    if (!submission?.gradingJobId) return;
    stop.mutate({ operationId: operations.take(`stop:${submission.gradingJobId}`), programId: form.programId, jobId: submission.gradingJobId, expectedRevision: programRevision, formId: form.id }, { onSuccess: () => setStopped(true) });
  };
  const resumeGrading = () => { void formAction(`resume:${form.id}:${submission?.pendingItems}`, (operationId) => resume.mutateAsync({ ...base, operationId })).then((ok) => ok && setStopped(false)); };

  return <div className="space-y-3">
    <BackLink label="Quizzes" onClick={onBack} />
    <section className="overflow-hidden rounded-2xl border border-border bg-surface shadow-sm">
      <header className={`${parchmentBand} px-5 py-6 sm:px-8`}>
        <div className={eyebrow}>Result · {practice ? 'Practice · ' : ''}{scopeKindLabel[kind]} · {form.title}</div>
        <div className="mt-4 flex flex-wrap items-center gap-6">
          <ScoreRing score={shown?.score ?? 0} passMark={practice ? null : form.passingScore} tone={headline.tone} label={shown ? percent(shown.score) : '—'} />
          <div className="min-w-0 flex-1">
            <h2 id="quiz-result-heading" ref={heading} tabIndex={-1} className={`flex items-center gap-2.5 font-serif text-[2rem] leading-tight outline-hidden ${headline.tone === 'passed' ? 'text-emerald-800 dark:text-emerald-200' : 'text-text-primary'}`}>
              <VerdictIcon tone={headline.tone} className="h-9 w-9" /> {headline.label}
            </h2>
            <p className="mt-1.5 text-base tabular-nums text-text-primary">
              {shown?.spoken && <span className="sr-only">{shown.spoken} · </span>}{shown?.line}
              {headline.detail && <span className="text-text-secondary"> · {headline.detail}</span>}
            </p>
            {passMarkWords(form) && <p className="mt-1 text-sm text-text-secondary">{passMarkWords(form)}</p>}
            {overridden > 0 && submission && <p className="mt-1 text-sm text-violet-900 dark:text-violet-200">Score includes your override ({plural(overridden, 'answer')} overridden by you){submission.scoreWithoutOverrides !== null ? ` · ${percent(submission.scoreWithoutOverrides)} without it` : ''}.</p>}
          </div>
          <dl className="grid shrink-0 gap-1 text-sm text-text-secondary sm:text-right">
            {!practice && <div><dt className="inline">Pass mark </dt><dd className="inline font-semibold text-text-primary">{percent(form.passingScore)}</dd></div>}
            <div><dt className="sr-only">Attempt</dt><dd>{form.retakeOfFormId ? 'Retake' : 'First attempt'} · {shortDate(form.submittedAt)}</dd></div>
          </dl>
        </div>
        <p className="mt-4 max-w-3xl text-xs leading-5 text-text-muted">
          {submission?.graderModel ? `Written answers: provisional, graded by ${submission.graderModel}. ` : ''}Keyed answers: graded against the form. This is evidence from one attempt, not a claim of mastery.
        </p>
        <div className="mt-5 flex flex-wrap items-center gap-2">
          {completion && !practice && submission?.passed === true && (completion.completed
            ? <span className="inline-flex min-h-11 items-center gap-1.5 px-3 text-sm font-medium text-emerald-800 dark:text-emerald-200"><CheckCircle2 size={16} aria-hidden="true" /> Lesson marked complete</span>
            : <button type="button" disabled={completion.pending} onClick={completion.onComplete} className={primaryPill}><CheckCircle2 size={15} aria-hidden="true" /> {completion.pending ? 'Saving…' : 'Mark lesson complete'}</button>)}
          {onRetake && <button type="button" disabled={retakePending} onClick={() => onRetake(practice ? 'practice' : 'graded')} className={submission?.passed === false ? primaryPill : quietPill}><RotateCcw size={15} aria-hidden="true" /> {retakePending ? 'Preparing…' : practice ? 'Practise again with fresh questions' : 'Retake with fresh questions'}</button>}
          {missedIds.length > 0 && <button type="button" disabled={misses.isPending} onClick={() => addToRecall(missedIds)} className={quietPill}><BookmarkPlus size={15} aria-hidden="true" /> Add {plural(missedIds.length, 'missed question')} to Recall</button>}
        </div>
        {recallNote && <p role="status" className="mt-3 text-sm text-text-secondary">{recallNote}</p>}
        {(retakeError || errorText(misses.error)) && <p role="alert" className="mt-3 text-sm text-rose-700 dark:text-rose-300">{retakeError ?? errorText(misses.error)}</p>}
      </header>
      <GradingBanner form={form} grading={grading} onStop={stopGrading} onResume={resumeGrading} stopping={stop.isPending} resuming={resume.isPending} error={errorText(stop.error) ?? errorText(resume.error)} />
      <ol aria-label="Your answers" className="divide-y divide-border">{form.items.map((item, index) => <QuizItemReview key={item.id} item={item} index={index}
        result={submission?.itemResults.find((result) => result.itemId === item.id) ?? null} grading={grading} practice={practice} sources={sources} actions={actions} />)}</ol>
    </section>
  </div>;
}
