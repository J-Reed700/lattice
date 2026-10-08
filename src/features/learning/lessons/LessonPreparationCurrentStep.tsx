import { Activity, LoaderCircle } from 'lucide-react';

import { PREPARATION_PHASES, preparationDuration } from '@/features/learning/lessons/lessonPreparationActivity';
import type { LearningGenerationJob } from '@/lib/bindings';

/** Current work stays visible even when the user collapses the historical details. */
export function LessonPreparationCurrentStep({ job, now }: { job: LearningGenerationJob; now: number }) {
  const activity = job.activity;
  if (!activity) return null;
  const phase = PREPARATION_PHASES[activity.phase];
  const phaseElapsed = Math.max(0, (now - activity.phaseStartedAt) / 1000);
  const idle = Math.max(0, (now - activity.lastActivityAt) / 1000);
  const followingChecks = activity.phase === 'repair' || activity.phase === 'research';
  const checkedAll = activity.checksTotal > 0 && activity.checksCompleted >= activity.checksTotal;
  const findings = `${activity.checksUnresolved} ${activity.checksUnresolved === 1 ? 'claim needed' : 'claims needed'} attention in the last pass`;
  return <div aria-label="Current preparation step" className="m-5 rounded-xl border border-accent/40 bg-background/80 p-5">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <p className="text-xs font-semibold uppercase tracking-wide text-accent">{activity.phase === 'publishing' ? 'Final step · Saving your lesson' : 'Lesson not ready yet'}</p>
      {activity.phaseStartedAt > 0 && <span className="text-xs tabular-nums text-text-secondary" aria-live="off">This step · {preparationDuration(phaseElapsed)}</span>}
    </div>
    <div aria-live="polite" aria-atomic="true" className="mt-3">
      <h4 className="text-lg font-semibold text-text-primary">{phase.title}</h4>
      {followingChecks && checkedAll && <p className="mt-2 text-sm leading-6 text-text-primary">Checking finished; {findings}. {activity.phase === 'repair' ? 'The affected sections are being corrected before another review.' : 'Additional evidence is being researched before another review.'}</p>}
      <p className="mt-3 wrap-break-word border-l-2 border-accent pl-3 text-base font-medium leading-6 text-text-primary">{job.progressMessage}</p>
    </div>
    <p className="mt-3 text-sm leading-6 text-text-secondary">{phase.detail}</p>
    <div className="mt-4 rounded-lg border border-border bg-background p-3 text-sm leading-6">
      <p className="flex items-center gap-2 font-medium text-text-primary">
        {activity.modelRunning ? <LoaderCircle size={16} className="shrink-0 animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" /> : <Activity size={16} className="shrink-0 text-accent" aria-hidden="true" />}
        {activity.modelRunning ? activity.responseCharacters > 0 ? 'Receiving model output' : 'Waiting for model output' : 'Waiting for the next progress update'}
      </p>
      {activity.lastActivityAt > 0 && <p className="mt-1 tabular-nums text-text-secondary" aria-live="off">Last reported progress · {preparationDuration(idle)} ago</p>}
      {activity.modelRunning && activity.responseCharacters > 0 && <p className="mt-1 tabular-nums text-text-secondary">{activity.responseCharacters.toLocaleString()} response characters received since the current requests started or retried.</p>}
      {activity.modelRunning && activity.modelAttempt > 1 && <p className="mt-1 text-text-secondary">Retrying the model request · attempt {activity.modelAttempt}. Partial responses are discarded before retrying.</p>}
      {idle >= 90 && <p className="mt-2 text-text-secondary">No new output or completed step for {preparationDuration(idle)}. {activity.modelRunning ? 'The request is still open, but the model has not reported more progress. ' : ''}You can pause and resume from saved work.</p>}
      {activity.modelRunning && activity.modelName && <p className="mt-2 break-all text-xs text-text-muted">Model: {activity.modelName}</p>}
    </div>
    <p className="mt-4 text-sm leading-6 text-text-secondary"><span className="font-semibold text-text-primary">Next: </span>{phase.next}</p>
  </div>;
}
