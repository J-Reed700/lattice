import { ArrowRight, BookOpen, Play, RotateCcw, Sparkles } from 'lucide-react';

import { coverageWords, freshnessWords, quizStatusWords } from '@/features/learning/quizzes/quizCopy';
import { VerdictChip } from '@/features/learning/quizzes/QuizVerdict';
import { primaryPill, quietPill } from '@/features/learning/quizzes/RunnerParts';
import type { QuizLauncher } from '@/features/learning/quizzes/useQuizLauncher';
import type { LearningQuizScopeKind, LearningQuizStatusDto } from '@/lib/bindings';

const startLabel: Record<LearningQuizScopeKind, string> = { lesson: 'Start quiz', module: 'Start test', course: 'Start exam' };
const prepareLabel: Record<LearningQuizScopeKind, string> = { lesson: 'Prepare questions', module: 'Prepare test', course: 'Prepare exam' };
const smallPrimary = primaryPill.replace('px-5', 'px-4');
const textLink = 'inline-flex min-h-11 items-center gap-1 rounded-full px-2 text-sm font-medium text-accent hover:underline focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50';

/** The buttons for one quiz's state. Nothing is ever locked: an unprepared lesson says how to get its quiz. */
export function QuizStatusActions({ status, launcher, lessonPrepared = true, onOpenLesson, onViewResult, retakeLabel = 'Retake' }: {
  status: LearningQuizStatusDto; launcher: QuizLauncher; lessonPrepared?: boolean; onOpenLesson?: () => void; onViewResult: (formId: string) => void;
  /** Rows say "Retake" beside a line on fresh questions; a callout says it on the button. */
  retakeLabel?: string;
}) {
  const busy = launcher.isBusy(status);
  const kind = status.scope.kind;
  const resultId = status.best?.formId ?? status.latest?.formId;
  if (status.activeFormId) return <>
    <button type="button" onClick={() => launcher.resume(status)} className={smallPrimary}><Play size={14} aria-hidden="true" /> Resume</button>
    <button type="button" disabled={busy} onClick={() => void launcher.startFresh(status)} className={textLink}>Start fresh</button>
  </>;
  if (status.bankStatus === 'preparing' && !status.attempts) return <button type="button" disabled className={quietPill}>Preparing…</button>;
  if (status.bankStatus === 'not_prepared' || status.bankStatus === 'failed') {
    if (kind === 'lesson' && !lessonPrepared) return onOpenLesson ? <button type="button" onClick={onOpenLesson} className={textLink}><BookOpen size={14} aria-hidden="true" /> Prepare the lesson</button> : null;
    return <button type="button" disabled={busy} onClick={() => void launcher.prepareBank(status)} className={quietPill}>{busy ? 'Starting…' : status.bankStatus === 'failed' ? 'Try again' : prepareLabel[kind]}</button>;
  }
  if (status.verdict === 'not_taken') return <>
    <button type="button" disabled={busy} onClick={() => void launcher.begin(status, 'graded')} className={smallPrimary}>{busy ? 'Starting…' : startLabel[kind]} <ArrowRight size={14} aria-hidden="true" /></button>
    <button type="button" disabled={busy} onClick={() => void launcher.begin(status, 'practice')} className={textLink}>Practice first</button>
  </>;
  return <>
    {resultId && <button type="button" onClick={() => onViewResult(resultId)} className={textLink}>{status.verdict === 'pending' ? 'View result' : 'Review'}</button>}
    {status.verdict !== 'pending' && <button type="button" disabled={busy} onClick={() => void launcher.retake(status)} className={status.verdict === 'passed' ? quietPill : smallPrimary}><RotateCcw size={14} aria-hidden="true" /> {busy ? 'Starting…' : retakeLabel}</button>}
  </>;
}

/** One quiz in the overview: its title, its verdict in words, best and latest, and what you can do. */
export function QuizStatusRow({ status, launcher, number, lessonPrepared, onOpenLesson, onViewResult, emphasis = false }: {
  status: LearningQuizStatusDto; launcher: QuizLauncher; number?: string; lessonPrepared?: boolean; onOpenLesson?: () => void;
  onViewResult: (formId: string) => void; emphasis?: boolean;
}) {
  const words = quizStatusWords(status);
  const fresh = status.verdict === 'passed' || status.verdict === 'not_passed' ? freshnessWords(status) : undefined;
  const error = launcher.errorFor(status);
  const topUp = Boolean(fresh && status.freshItems < status.formSize && status.bankStatus === 'ready');
  return <div id={`quiz-${status.scope.kind}-${status.scope.id ?? 'course'}`} tabIndex={-1} className="grid gap-3 py-4 outline-hidden focus-visible:ring-2 focus-visible:ring-accent sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
    <div className="min-w-0">
      <div className="flex items-baseline gap-2">
        {number && <span className="w-6 shrink-0 font-serif text-base tabular-nums text-accent">{number}</span>}
        <span className={`min-w-0 ${emphasis ? 'font-serif text-lg text-text-primary' : 'text-sm font-medium text-text-primary'}`}>{status.title}</span>
      </div>
      <div className={`mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 ${number ? 'pl-8' : ''}`}>
        <VerdictChip words={words} />
        {words.detail && <span className="text-xs text-text-muted">{words.detail}</span>}
      </div>
      <p className={`mt-1 text-xs leading-5 text-text-muted ${number ? 'pl-8' : ''}`}>
        {status.bankStatus === 'ready' && `${status.formSize} questions · about ${status.expectedMinutes} min`}
        {status.coverage && status.scope.kind !== 'lesson' && `${status.bankStatus === 'ready' ? ' · ' : ''}${coverageWords(status.coverage, status.scope.kind)}`}
      </p>
      {fresh && <p className={`mt-0.5 text-xs leading-5 text-text-muted ${number ? 'pl-8' : ''}`}>{fresh}{topUp && <> <button type="button" onClick={() => void launcher.prepareBank(status, true)} className="inline-flex items-center gap-1 font-medium text-accent hover:underline"><Sparkles size={12} aria-hidden="true" />Prepare more questions</button></>}</p>}
      {error && <p role="alert" className={`mt-1 text-xs text-rose-700 dark:text-rose-300 ${number ? 'pl-8' : ''}`}>{error}</p>}
    </div>
    <div className={`flex flex-wrap items-center gap-1.5 sm:justify-end ${number ? 'pl-8 sm:pl-0' : ''}`}>
      <QuizStatusActions status={status} launcher={launcher} lessonPrepared={lessonPrepared} onOpenLesson={onOpenLesson} onViewResult={onViewResult} />
    </div>
  </div>;
}
