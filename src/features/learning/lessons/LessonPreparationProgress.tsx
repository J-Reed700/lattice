import { useEffect, useId, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ChevronDown, CirclePause, ClipboardCheck, LoaderCircle, Save } from 'lucide-react';

import { useCancelLearningGenerationJob, useRetryLearningGenerationJob } from '@/features/learning/curriculum/useLearningPlan';
import { PREPARATION_PHASES, preparationDuration, preparationState } from '@/features/learning/lessons/lessonPreparationActivity';
import { LessonPreparationCurrentStep } from '@/features/learning/lessons/LessonPreparationCurrentStep';
import { claimsNeedYou, jobTallyLine } from '@/features/learning/lessons/review/claimReviewCopy';
import { isWaitingForYou, reviewHold, type ReviewHold } from '@/features/learning/lessons/review/claimReviewPresentation';
import { useLessonClaimReview } from '@/features/learning/lessons/review/useLessonClaimReview';
import { useEvidenceWaitsForIndex } from '@/features/learning/sources/useCourseIndexStatus';
import { LEARNING_PROGRAMS_KEY, learningProgramKey } from '@/features/learning/workspace/useLearningStudio';
import type { LearningGenerationJob } from '@/lib/bindings';

const workflow = ['References', 'Draft', 'Teaching review', 'Coverage & examples', 'Fact checks & repairs', 'Publish'];

/** "Re-checking 81 claims against 72 new references"; the claim count once saved checks are compared. */
function recheckLine(references: number, claims: number | null): string {
  const what = claims == null ? 'claims' : `${claims} ${claims === 1 ? 'claim' : 'claims'}`;
  return `Re-checking ${what} against ${references} new ${references === 1 ? 'reference' : 'references'}`;
}

/** A finished job's lessons are new course content: reload the course. */
export function useReloadWhenPrepared(job: LearningGenerationJob | undefined, programId: string) {
  const client = useQueryClient();
  useEffect(() => {
    if (job?.status !== 'completed') return;
    void client.invalidateQueries({ queryKey: learningProgramKey(programId) });
    void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
  }, [client, job?.id, job?.status, programId]);
}

export type ReviewOpener = (programId: string, lessonId: string) => void;

const HOLD_WORDS: Record<Exclude<ReviewHold, 'claims'>, string> = {
  audit: 'waiting for the independent check',
  audit_stop: 'waiting for the independent check',
  resolved: 'everything is resolved · resume to publish',
};

/** One held lesson: "<title> — 33 claims need you · Review". Never a failure, never the claims themselves. */
function HeldLessonLine({ programId, lessonId, title, onReview }: { programId: string; lessonId: string; title: string; onReview?: ReviewOpener }) {
  const review = useLessonClaimReview(programId, lessonId).data;
  const waiting = review?.claims.filter(isWaitingForYou).length ?? null;
  const hold = review && waiting != null ? reviewHold(review, waiting) : 'claims';
  const state = review?.state === 'stopped' ? 'stopped with a cause to fix'
    : hold === 'claims' ? waiting ? claimsNeedYou(waiting) : 'waiting for you' : HOLD_WORDS[hold];
  return <li className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5">
    <p className="flex min-w-0 items-center gap-2 text-sm text-text-primary">
      <ClipboardCheck size={16} className="shrink-0 text-accent" aria-hidden="true" />
      <span className="min-w-0 wrap-break-word"><span className="font-medium">{title}</span> <span className="text-text-secondary">— {state}</span></span>
    </p>
    {onReview && <button type="button" onClick={() => onReview(programId, lessonId)} className="min-h-9 rounded-full bg-accent px-4 text-xs font-semibold text-accent-fg">Review</button>}
  </li>;
}

/**
 * The lesson preparation job on the course page (UI design §2.2): one status
 * line, the progress bar, Pause/Resume, and everything else behind Details.
 * A lesson held for review is a line that leads to the review screen.
 */
export function LessonPreparationStatus({ job, pending, programId, revision, error, expanded: heldExpanded, onExpandedChange, lessonTitles, onReview }: {
  job?: LearningGenerationJob; pending: boolean; programId: string; revision: number; error?: string;
  // A parent that unmounts the panel (the course tabs) holds this, so an opened Details survives a tab switch.
  expanded?: boolean; onExpandedChange?: (expanded: boolean) => void;
  /** Lesson titles by id, for lessons the job holds for review. */
  lessonTitles?: Record<string, string>;
  /** Opens a held lesson's review screen. */
  onReview?: ReviewOpener;
}) {
  const cancel = useCancelLearningGenerationJob();
  const retry = useRetryLearningGenerationJob();
  // Retrying or resuming reads the course's evidence; pausing never waits.
  // The reason, with the time left, sits beside the held button.
  const waitsForIndex = useEvidenceWaitsForIndex(programId);
  const indexWaitId = useId();
  const [now, setNow] = useState(Date.now());
  const [ownExpanded, setOwnExpanded] = useState(false);
  const expanded = heldExpanded ?? ownExpanded;
  const setExpanded = onExpandedChange ?? setOwnExpanded;
  const detailsId = useId();
  // Cancelling a lesson attempt preserves its draft/checkpoints and prevents
  // automatic dispatch (`paused`). Retrying it resumes that saved work in a new attempt.
  const { active, paused, queued, reconnecting, phase, held } = preparationState(job, pending);
  const activity = job?.activity;
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active]);
  useReloadWhenPrepared(job, programId);
  // Lessons held for review are not a failure: one line each, leading to the review screen (§6.1).
  if (job && held.length > 0) {
    return <section aria-label="Lesson preparation" className="my-3 rounded-xl border border-accent/30 bg-accent/5 px-4 py-3">
      <ul className="space-y-2">{held.map((lessonId) => <HeldLessonLine key={lessonId} programId={programId} lessonId={lessonId}
        title={lessonTitles?.[lessonId] ?? (held.length === 1 ? activity?.lessonTitle : null) ?? 'A lesson'} onReview={onReview} />)}</ul>
    </section>;
  }
  if (!active && (!job || job.status === 'completed')) return null;
  const elapsed = Math.max(0, (now - (job?.startedAt ?? job?.createdAt ?? now)) / 1000);
  const request = () => ({ operationId: crypto.randomUUID(), programId, jobId: job!.id, expectedRevision: revision });
  const actionError = cancel.error ?? retry.error;
  const hasChecks = activity && activity.checksTotal > 0;
  const remaining = hasChecks ? Math.max(0, activity.checksTotal - activity.checksCompleted) : 0;
  // The source check's claims were supported by the checker but not by
  // strong enough sources: they count apart from both.
  const supported = hasChecks ? Math.max(0, activity.checksCompleted - activity.checksUnresolved - activity.checksSentBack) : 0;
  const restoringChecks = active && activity?.phase === 'evidence' && activity.modelChecksTotal == null;
  const currentChecks = active && !queued && activity?.phase === 'evidence';
  // After every claim's first check, unsupported claims are checked against
  // further evidence pages. Those checks move claims between counts.
  const deeper = (currentChecks && activity?.deeperEvidence) || null;
  // Research saved references; this step of the pass checks claims again against them.
  const recheck = currentChecks && activity?.newReferences != null ? recheckLine(activity.newReferences, activity.modelChecksTotal) : null;
  const bar = deeper
    ? { label: `Unsupported claims checked against evidence page ${deeper.page}`, value: deeper.checked, max: deeper.claims, text: `${deeper.checked} of ${deeper.claims} checked on evidence page ${deeper.page}` }
    : hasChecks ? { label: 'Claim checks this pass', value: activity.checksCompleted, max: activity.checksTotal, text: `${activity.checksCompleted} of ${activity.checksTotal} checked` } : null;
  const showCurrentStep = active && !queued && Boolean(activity);
  const lesson = activity?.lessonTitle;
  const step = phase ? workflow[phase.group] : null;
  const statusLine = reconnecting ? `Preparation will resume automatically${lesson ? ` · ${lesson}` : ''}`
    : active ? `Preparing ${lesson ?? 'your lesson'} · ${queued ? 'Queued' : step ?? 'Starting'}`
    : paused ? `Lesson preparation paused${phase ? ` · at ${phase.title.toLowerCase()}` : ''}${lesson ? ` · ${lesson}` : ''}`
    : `Lesson preparation ${job?.status}${lesson ? ` · ${lesson}` : ''}`;
  return <section aria-label="Lesson preparation" className="my-3 rounded-xl border border-accent/30 bg-accent/5 px-4 py-3">
    <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
      <p className="flex min-w-0 flex-1 items-center gap-2 text-sm text-text-primary">
        {active && !reconnecting ? <LoaderCircle size={16} className="shrink-0 animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" /> : <CirclePause size={16} className="shrink-0 text-accent" aria-hidden="true" />}
        <span role="status" className="min-w-0 wrap-break-word">{statusLine}</span>
        {active && <><span aria-hidden="true" className="text-text-muted">·</span><span className="shrink-0 tabular-nums text-text-secondary" aria-live="off">{reconnecting ? 'Waiting to retry' : queued ? 'Waiting for a slot' : preparationDuration(elapsed)}</span></>}
      </p>
      {job && <div className="flex flex-wrap items-center gap-2">
        <button type="button" disabled={cancel.isPending || retry.isPending || (!active && Boolean(waitsForIndex))} aria-describedby={!active && waitsForIndex ? indexWaitId : undefined} onClick={() => {
          cancel.reset(); retry.reset();
          if (active) cancel.mutate(request()); else retry.mutate(request());
        }} className="min-h-9 rounded-full border border-accent/40 bg-background px-3 text-xs font-medium text-text-primary disabled:opacity-50">{cancel.isPending ? 'Pausing…' : retry.isPending ? paused ? 'Resuming…' : 'Retrying…' : active ? 'Pause lesson preparation' : paused ? 'Resume lesson preparation' : 'Retry lesson preparation'}</button>
        <button type="button" aria-expanded={expanded} aria-controls={detailsId} onClick={() => setExpanded(!expanded)} className="inline-flex min-h-9 items-center gap-1 rounded-full px-3 text-xs font-medium text-text-secondary hover:bg-background">Details<ChevronDown size={14} className={expanded ? 'rotate-180' : ''} aria-hidden="true" /></button>
      </div>}
    </div>
    {!active && waitsForIndex && <p id={indexWaitId} className="mt-1 text-xs tabular-nums text-text-secondary">{waitsForIndex}</p>}
    {currentChecks && !restoringChecks && bar && <div role="progressbar" aria-label={bar.label} aria-valuenow={bar.value} aria-valuemin={0} aria-valuemax={bar.max} aria-valuetext={`${bar.text}; ${activity.checksUnresolved} need attention; ${activity.checksReused} saved checks reused`} className="mt-2.5 h-1.5 overflow-hidden rounded-full bg-border"><div className="h-full rounded-full bg-accent transition-[width] motion-reduce:transition-none" style={{ width: `${Math.min(100, bar.value / bar.max * 100)}%` }} /></div>}
    {actionError && <p role="alert" className="mt-2 text-xs text-rose-700">{actionError.message}</p>}
    {!active && !paused && <p role="alert" className="mt-2 text-sm leading-6 text-rose-700">{job?.error || error || 'Preparation stopped. Your draft and completed checkpoints remain saved.'}</p>}
    {expanded && <div id={detailsId} className="mt-3 space-y-4 border-t border-accent/15 pt-3">
      {showCurrentStep && job ? <LessonPreparationCurrentStep job={job} now={now} /> : <div>
        <h4 className="text-base font-semibold text-text-primary">{paused ? phase ? `Paused at: ${phase.title}` : 'Saved work is available' : reconnecting ? 'Preparation interrupted' : queued ? 'Waiting for a preparation slot' : phase?.title ?? (active ? 'Starting lesson preparation' : 'Saved work is available')}</h4>
        <p className="mt-2 text-sm leading-6 text-text-secondary">{paused ? 'Preparation stays paused until you choose Resume, even after reopening the app. Your draft and completed checkpoints are saved.' : reconnecting ? job?.error : queued ? 'Your request is saved. Preparation continues automatically when a worker is available.' : phase?.detail ?? 'Preparation includes writing, reviewing the teaching and answer keys, and checking factual claims against saved references.'}</p>
        {reconnecting && phase && <p className="mt-2 text-xs leading-5 text-text-secondary">Last step: {phase.title}. When preparation resumes, it reuses completed checks and repeats unfinished work.</p>}
        {!paused && <p className="mt-2 wrap-break-word text-xs leading-5 text-text-muted" aria-live="polite">{job?.progressMessage || 'Saving the preparation request…'}</p>}
        {paused && <p className="mt-2 text-xs leading-5 text-text-secondary">Lesson writing and fact-checking use the main model in Settings. Changing only the utility model does not change these checks. Switching the main model currently starts drafting and verification again.</p>}
      </div>}
      {job && job.progressTotal > 1 && <p className="text-xs text-text-muted">{job.progressCompleted}/{job.progressTotal} lessons staged · Publication follows verification.</p>}
      <ol aria-label="Preparation workflow" className="grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-6">
        {workflow.map((label, index) => <li key={label} aria-current={active && !queued && phase?.group === index ? 'step' : undefined} className={`rounded-lg border px-3 py-2 text-xs ${active && !queued && phase?.group === index ? 'border-accent/50 bg-accent/10 font-semibold text-text-primary' : 'border-border text-text-muted'}`}><span className="mr-1.5 tabular-nums opacity-60">{index + 1}</span>{label}</li>)}
      </ol>
      {hasChecks && <div className="rounded-xl border border-border bg-background/70 p-4">
        <div className="flex flex-wrap justify-between gap-2 text-xs"><span className="font-semibold text-text-primary">{recheck ?? (activity.phase === 'evidence' ? 'Claim checks this pass' : 'Last verification pass')} · Pass {activity.verificationPass}</span><span className="tabular-nums text-text-secondary">{restoringChecks ? `${activity.checksTotal} lesson claims` : `${activity.checksCompleted} / ${activity.checksTotal} checked`}</span></div>
        {restoringChecks ? <p className="mt-3 text-sm leading-6 text-text-secondary" role="status">Looking for saved checks to reuse. The number needing model review will appear once all saved results have been compared with the current evidence.</p>
          : activity.modelChecksTotal != null && currentChecks && <p className="mt-3 text-sm leading-6 text-text-secondary">{activity.checksReused} saved checks reused. {deeper ? `Every claim has had its first check. Checking ${deeper.claims} unsupported claims against evidence page ${deeper.page}; ${deeper.claims - deeper.checked} left.` : remaining === 0 ? 'Checking finished. Preparing the next step.' : `${remaining} of ${activity.modelChecksTotal} model checks remaining in this pass.`}</p>}
        <dl className={`mt-3 grid grid-cols-2 gap-3 text-xs ${currentChecks ? 'sm:grid-cols-5' : 'sm:grid-cols-4'}`}>
          <div><dt className="text-text-muted">Supported</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{supported}</dd></div>
          <div><dt className="text-text-muted">{currentChecks ? 'Need attention' : 'Needed attention in this pass'}</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{activity.checksUnresolved}</dd></div>
          <div><dt className="text-text-muted">Sent back for stronger sources</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{activity.checksSentBack}</dd></div>
          {currentChecks && <div><dt className="text-text-muted">{deeper ? `Left on evidence page ${deeper.page}` : recheck ? 'Still to re-check' : 'Still to check'}</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{restoringChecks ? 'Calculating…' : deeper ? deeper.claims - deeper.checked : remaining}</dd></div>}
          <div><dt className="text-text-muted">Saved checks reused</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{activity.checksReused}</dd></div>
        </dl>
      </div>}
      {activity?.claims && <div className="rounded-xl border border-border bg-background/70 p-4 text-xs leading-5">
        <h4 className="font-semibold text-text-primary">Claims</h4>
        <p className="mt-1 tabular-nums text-text-secondary">{jobTallyLine(activity.claims)}</p>
      </div>}
      <p className="flex flex-wrap items-center gap-2 text-xs leading-5 text-text-secondary"><Save size={14} className="shrink-0 text-accent" aria-hidden="true" /><span className="font-medium text-text-primary">{activity?.lastCheckpointAt ? `Checkpoint saved at ${new Date(activity.lastCheckpointAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}` : 'Progress saves automatically'}.</span>
        <span>{active ? 'Preparation can take hours; quitting pauses it, and it resumes from saved checkpoints when you reopen the app.' : paused ? 'Resume uses your current model settings and reuses valid saved checkpoints.' : 'Retry continues from valid saved checkpoints.'}</span></p>
      {activity && activity.recentSteps.length > 1 && <details className="rounded-xl border border-border px-4 py-3 text-xs">
        <summary className="cursor-pointer font-medium text-text-primary">Recent activity</summary>
        <ol className="mt-3 space-y-2 text-text-secondary">{[...activity.recentSteps].reverse().map((entry, index) => <li key={`${entry.startedAt}-${index}`} className="flex flex-wrap justify-between gap-2"><span>{PREPARATION_PHASES[entry.phase].title}</span><time className="tabular-nums text-text-muted" dateTime={new Date(entry.startedAt).toISOString()}>{new Date(entry.startedAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}</time></li>)}</ol>
      </details>}
    </div>}
  </section>;
}
