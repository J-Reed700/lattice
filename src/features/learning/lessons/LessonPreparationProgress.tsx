import { useEffect, useId, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ChevronDown, CirclePause, LoaderCircle, Save } from 'lucide-react';

import { useCancelLearningGenerationJob, useRetryLearningGenerationJob } from '@/features/learning/curriculum/useLearningPlan';
import { PREPARATION_PHASES, preparationDuration } from '@/features/learning/lessons/lessonPreparationActivity';
import { LessonPreparationCurrentStep } from '@/features/learning/lessons/LessonPreparationCurrentStep';
import { LEARNING_PROGRAMS_KEY, learningProgramKey } from '@/features/learning/workspace/useLearningStudio';
import type { LearningGenerationJob } from '@/lib/bindings';

const workflow = ['References', 'Draft', 'Teaching review', 'Coverage & examples', 'Fact checks & repairs', 'Publish'];

export function LessonPreparationProgress({ job, pending, programId, revision, error }: {
  job?: LearningGenerationJob; pending: boolean; programId: string; revision: number; error?: string;
}) {
  const client = useQueryClient();
  const cancel = useCancelLearningGenerationJob();
  const retry = useRetryLearningGenerationJob();
  const [now, setNow] = useState(Date.now());
  const [expanded, setExpanded] = useState(true);
  const detailsId = useId();
  const active = job ? ['pending', 'running'].includes(job.status) : pending;
  // Cancelling a lesson attempt preserves its draft/checkpoints and prevents
  // automatic dispatch. Retrying it resumes that saved work in a new attempt.
  const paused = job?.status === 'cancelled';
  const activity = job?.activity;
  const phase = activity ? PREPARATION_PHASES[activity.phase] : undefined;
  const queued = job?.status === 'pending';
  const reconnecting = queued && Boolean(job.error);
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active]);
  useEffect(() => {
    if (job?.status !== 'completed') return;
    void client.invalidateQueries({ queryKey: learningProgramKey(programId) });
    void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
  }, [client, job?.id, job?.status, programId]);
  if (!active && (!job || job.status === 'completed')) return null;
  const elapsed = Math.max(0, (now - (job?.startedAt ?? job?.createdAt ?? now)) / 1000);
  const request = () => ({ operationId: crypto.randomUUID(), programId, jobId: job!.id, expectedRevision: revision });
  const actionError = cancel.error ?? retry.error;
  const hasChecks = activity && activity.checksTotal > 0;
  const remaining = hasChecks ? Math.max(0, activity.checksTotal - activity.checksCompleted) : 0;
  const supported = hasChecks ? Math.max(0, activity.checksCompleted - activity.checksUnresolved) : 0;
  const restoringChecks = active && activity?.phase === 'evidence' && activity.modelChecksTotal == null;
  const currentChecks = active && !queued && activity?.phase === 'evidence';
  const showCurrentStep = active && !queued && Boolean(activity);
  return <section aria-label="Lesson preparation progress" className="my-4 overflow-hidden rounded-2xl border border-accent/30 bg-accent/5">
    <div className="border-b border-accent/15 p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-text-primary" role="status">
            {active && !reconnecting ? <LoaderCircle size={18} className="shrink-0 animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" /> : <CirclePause size={18} className="shrink-0 text-accent" aria-hidden="true" />}
            {reconnecting ? 'Preparation will resume automatically' : active ? 'Preparing your lesson' : paused ? 'Lesson preparation paused' : `Lesson preparation ${job?.status}`}
          </h3>
          {activity?.lessonTitle && <p className="mt-2 wrap-break-word font-serif text-lg text-text-primary">{activity.lessonTitle}</p>}
        </div>
        {job && <div className="flex flex-wrap items-center gap-3">
          {active && <span className="rounded-full bg-background px-3 py-1.5 text-xs tabular-nums text-text-secondary" aria-live="off">{reconnecting ? 'Waiting to retry' : queued ? 'Queued' : `This run · ${preparationDuration(elapsed)}`}</span>}
          <button type="button" disabled={cancel.isPending || retry.isPending} onClick={() => {
            cancel.reset(); retry.reset();
            if (active) cancel.mutate(request()); else retry.mutate(request());
          }} className="rounded-full border border-accent/40 bg-background px-4 py-2 text-sm font-medium text-text-primary disabled:opacity-50">{cancel.isPending ? 'Pausing…' : retry.isPending ? paused ? 'Resuming…' : 'Retrying…' : active ? 'Pause lesson preparation' : paused ? 'Resume lesson preparation' : 'Retry lesson preparation'}</button>
        </div>}
      </div>
      {!active && !paused && job?.finishedAt != null && <p className="mt-2 text-xs text-text-muted">Last attempt stopped at {new Date(job.finishedAt).toLocaleString()}. This is the saved result of that attempt.</p>}
      <p className="mt-3 text-xs leading-5 text-text-secondary">{active ? 'You can pause to change models. Your draft and completed checks stay saved.' : paused ? 'Change your model in Settings, then choose Resume when you are ready.' : 'Retry continues from valid saved checkpoints.'}</p>
      {actionError && <p role="alert" className="mt-3 text-xs text-rose-700">{actionError.message}</p>}
      <button type="button" aria-expanded={expanded} aria-controls={detailsId} onClick={() => setExpanded(!expanded)} className="mt-3 inline-flex items-center gap-1 rounded-full border border-border bg-background px-3 py-1.5 text-xs font-medium text-text-secondary"><ChevronDown size={14} className={expanded ? 'rotate-180' : ''} aria-hidden="true" />{expanded ? 'Hide details' : 'Show details'}</button>
      {!expanded && !showCurrentStep && <p className="mt-3 text-sm leading-6 text-text-secondary" aria-live="polite">{paused ? `Paused${phase ? ` at: ${phase.title}` : ''}. Saved work is available.` : `${phase?.title ?? 'Preparing your lesson'} · ${job?.progressMessage || 'Request saved'}`}</p>}
      {expanded && job && job.progressTotal > 1 && <p className="mt-3 text-xs text-text-muted">{job.progressCompleted}/{job.progressTotal} lessons staged · Publication follows verification.</p>}
      {expanded && <ol aria-label="Preparation workflow" className="mt-5 grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-6">
        {workflow.map((label, index) => <li key={label} aria-current={active && !queued && phase?.group === index ? 'step' : undefined} className={`rounded-lg border px-3 py-2 text-xs ${active && !queued && phase?.group === index ? 'border-accent/50 bg-accent/10 font-semibold text-text-primary' : 'border-border text-text-muted'}`}><span className="mr-1.5 tabular-nums opacity-60">{index + 1}</span>{label}</li>)}
      </ol>}
    </div>
    {showCurrentStep && job && <LessonPreparationCurrentStep job={job} now={now} />}
    <div id={detailsId} hidden={!expanded} className="space-y-4 p-5">
      {!showCurrentStep && <div>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h4 className="text-base font-semibold text-text-primary">{paused ? phase ? `Paused at: ${phase.title}` : 'Saved work is available' : reconnecting ? 'Preparation interrupted' : queued ? 'Waiting for a preparation slot' : phase?.title ?? (active ? 'Starting lesson preparation' : 'Saved work is available')}</h4>
        </div>
        <p className="mt-2 text-sm leading-6 text-text-secondary">{paused ? 'Preparation stays paused until you choose Resume, even after reopening the app. Your draft and completed checkpoints are saved.' : reconnecting ? job.error : queued ? 'Your request is saved. Preparation continues automatically when a worker is available.' : phase?.detail ?? 'Preparation includes writing, reviewing the teaching and answer keys, and checking factual claims against saved references.'}</p>
        {reconnecting && phase && <p className="mt-2 text-xs leading-5 text-text-secondary">Last step: {phase.title}. When preparation resumes, it reuses completed checks and repeats unfinished work.</p>}
        {!paused && <p className="mt-2 wrap-break-word text-xs leading-5 text-text-muted" aria-live="polite">{job?.progressMessage || 'Saving the preparation request…'}</p>}
        {paused && <p className="mt-2 text-xs leading-5 text-text-secondary">Lesson writing and fact-checking use the main model in Settings. Changing only the utility model does not change these checks. Switching the main model currently starts drafting and verification again.</p>}
      </div>}
      {hasChecks && <div className="rounded-xl border border-border bg-background/70 p-4">
        <div className="flex flex-wrap justify-between gap-2 text-xs"><span className="font-semibold text-text-primary">{activity.phase === 'evidence' ? 'Claim checks this pass' : 'Last verification pass'} · Pass {activity.verificationPass}</span><span className="tabular-nums text-text-secondary">{restoringChecks ? `${activity.checksTotal} lesson claims` : `${activity.checksCompleted} / ${activity.checksTotal} checked`}</span></div>
        {restoringChecks ? <p className="mt-3 text-sm leading-6 text-text-secondary" role="status">Looking for saved checks to reuse. The number needing model review will appear once all saved results have been compared with the current evidence.</p> : <>
          {activity.modelChecksTotal != null && currentChecks && <p className="mt-3 text-sm leading-6 text-text-secondary">{activity.checksReused} saved checks reused. {remaining === 0 ? 'Checking finished. Preparing the next step.' : `${remaining} of ${activity.modelChecksTotal} model checks remaining in this pass.`}</p>}
          {currentChecks && <div role="progressbar" aria-label="Claim checks this pass" aria-valuenow={activity.checksCompleted} aria-valuemin={0} aria-valuemax={activity.checksTotal} aria-valuetext={`${activity.checksCompleted} of ${activity.checksTotal} checked; ${activity.checksUnresolved} need attention; ${activity.checksReused} saved checks reused`} className="mt-3 h-2 overflow-hidden rounded-full bg-border"><div className="h-full rounded-full bg-accent transition-[width] motion-reduce:transition-none" style={{ width: `${Math.min(100, activity.checksCompleted / activity.checksTotal * 100)}%` }} /></div>}
        </>}
        <dl className={`mt-3 grid grid-cols-2 gap-3 text-xs ${currentChecks ? 'sm:grid-cols-4' : 'sm:grid-cols-3'}`}>
          <div><dt className="text-text-muted">Supported</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{supported}</dd></div>
          <div><dt className="text-text-muted">{currentChecks ? 'Need attention' : 'Needed attention in this pass'}</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{activity.checksUnresolved}</dd></div>
          {currentChecks && <div><dt className="text-text-muted">Still to check</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{restoringChecks ? 'Calculating…' : remaining}</dd></div>}
          <div><dt className="text-text-muted">Saved checks reused</dt><dd className="mt-1 text-lg tabular-nums text-text-primary">{activity.checksReused}</dd></div>
        </dl>
        <p className="mt-3 text-xs leading-5 text-text-muted">Checked means reviewed, not necessarily passed. Claims needing attention lead to research or repair. Completed checks keep their saved evidence. New references reopen checks only when new evidence is retrieved for that claim; changed content or sources also need review. Unchanged comparisons are reused.</p>
      </div>}
      {!active && !paused && <p role="alert" className="rounded-xl bg-rose-500/10 p-4 text-sm leading-6 text-rose-700">{job?.error || error || 'Preparation stopped. Your draft and completed checkpoints remain saved.'}</p>}
      <div className="rounded-xl border border-accent/20 p-4 text-xs leading-5 text-text-secondary">
        <p className="flex items-center gap-2 font-medium text-text-primary"><Save size={15} aria-hidden="true" />{activity?.lastCheckpointAt ? `Checkpoint saved at ${new Date(activity.lastCheckpointAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}` : 'Progress saves automatically'}</p>
        <p className="mt-2">{active ? 'Thorough preparation can take hours, depending on your model and material. You can use other parts of Lattice while it runs. Quitting pauses preparation; it resumes from saved checkpoints when you reopen the app. Choose Pause above to keep it paused. Unfinished checks may run again.' : paused ? 'Resume uses your current model settings and reuses valid saved checkpoints. Unfinished requests repeat, and checks affected by a model, content, source or verification-rule change may run again.' : 'Your completed checkpoints remain saved. Choose Retry to continue; reopening the app does not restart a failed job. Work affected by changed models, content, sources or verification rules is checked again.'}</p>
      </div>
      {activity && activity.recentSteps.length > 1 && <details className="rounded-xl border border-border px-4 py-3 text-xs">
        <summary className="cursor-pointer font-medium text-text-primary">Recent activity</summary>
        <ol className="mt-3 space-y-2 text-text-secondary">{[...activity.recentSteps].reverse().map((step, index) => <li key={`${step.startedAt}-${index}`} className="flex flex-wrap justify-between gap-2"><span>{PREPARATION_PHASES[step.phase].title}</span><time className="tabular-nums text-text-muted" dateTime={new Date(step.startedAt).toISOString()}>{new Date(step.startedAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}</time></li>)}</ol>
      </details>}
    </div>
  </section>;
}
