import { useEffect, useId, useRef, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { Check, ChevronDown, ChevronUp, Clock3, LoaderCircle, X } from 'lucide-react';

import { learningClaimReviewKey } from '@/features/learning/api/learningQueryKeys';
import { claimsNeedYou } from '@/features/learning/lessons/review/claimReviewCopy';
import { isWaitingForYou } from '@/features/learning/lessons/review/claimReviewPresentation';
import VaultAPI from '@/lib/api';
import type { LessonClaimReviewDto } from '@/lib/bindings';
import { queryClient } from '@/lib/queryClient';
import { dismissStudyActivity, EMPTY_STUDY_ACTIVITY, STUDY_ACTIVITY_KEY, type StudyActivity, type StudyDestination } from '@/lib/studyActivity';

function duration(milliseconds: number) {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

/**
 * Claims waiting for the user in the lesson a finished request points at; 0
 * when it points at no lesson, or the lesson is not held for review.
 */
function useClaimsWaitingIn(destination: StudyDestination, finished: boolean): number {
  const { programId = '', lessonId = '' } = destination;
  const { data } = useQuery<LessonClaimReviewDto | null>({
    queryKey: learningClaimReviewKey(programId, lessonId),
    queryFn: async () => {
      const result = await VaultAPI.getLearningLessonClaimReview(programId, lessonId);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    enabled: finished && Boolean(programId && lessonId),
    retry: false,
  }, queryClient);
  if (!data || (data.state !== 'needs_review' && data.state !== 'stopped')) return 0;
  return data.claims.filter(isWaitingForYou).length;
}

/** One line per request: what it is, for which course, how long, and where it leads. */
function ActivityCard({ activity, now, onOpen }: { activity: StudyActivity; now: number; onOpen: (destination: StudyDestination) => void }) {
  const pending = activity.status === 'pending';
  const failed = activity.status === 'failed';
  const destination = activity.destination;
  const waiting = useClaimsWaitingIn(destination, !pending && !failed);
  const outcome = pending ? 'Waiting for the result' : failed ? 'Request failed' : waiting > 0 ? claimsNeedYou(waiting) : 'Request completed';
  return <article aria-label={activity.title} className="rounded-xl border border-border-default bg-surface px-4 py-3 shadow-xs">
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
      <p className="flex min-w-0 flex-1 items-center gap-2 text-sm">
        {pending ? <LoaderCircle size={14} className="shrink-0 animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" /> : !failed ? <Check size={14} className="shrink-0 text-accent" aria-hidden="true" /> : null}
        <span className="min-w-0 truncate"><span className="font-semibold text-text-primary">{activity.title}</span>{activity.courseTitle && <span className="text-text-muted"> · {activity.courseTitle}</span>}</span>
      </p>
      <span className="shrink-0 text-xs tabular-nums text-text-muted" aria-live="off">{pending ? 'Elapsed' : 'Took'} {duration((activity.finishedAt ?? now) - activity.startedAt)}</span>
      <span role="status" className={`shrink-0 text-xs ${failed ? 'font-medium text-danger-fg' : waiting > 0 ? 'font-medium text-accent' : 'text-text-secondary'}`}>{outcome}</span>
      <span className="flex shrink-0 items-center gap-1.5">
        {waiting > 0
          ? <button type="button" onClick={() => onOpen({ ...destination, tab: 'lessons', review: 'claims' })} className="min-h-8 rounded-lg bg-accent px-3 text-xs font-semibold text-accent-fg shadow-xs transition hover:brightness-95 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">Review</button>
          : (destination.programId || destination.flashcards) && <button type="button" onClick={() => onOpen(destination)} className="min-h-8 rounded-lg border border-border-default bg-surface px-3 text-xs font-medium text-text-primary shadow-xs transition hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">{failed ? 'Return to task to retry' : pending ? 'Open task' : 'View result'}</button>}
        {!pending && <button type="button" onClick={() => dismissStudyActivity(activity.id)} className="min-h-8 rounded-lg px-3 text-xs text-text-muted transition hover:bg-surface-raised hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">Dismiss</button>}
      </span>
    </div>
    {failed && <p role="alert" className="mt-2 rounded-lg border border-border-default bg-danger-muted/30 p-2.5 text-xs leading-relaxed text-danger-fg">{activity.error}</p>}
    <details key={activity.status} className="mt-1 text-xs text-text-secondary">
      <summary className="cursor-pointer rounded-md py-1 font-medium text-accent hover:underline focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">Task details</summary>
      <div className="mt-2 rounded-lg border border-border-subtle bg-surface-sunken p-3 text-xs leading-relaxed">
        <p className="leading-5 text-text-secondary">{pending ? activity.detail : failed ? 'The request did not finish. Its error is shown above.' : 'The app has returned a result. Open the task to review it.'}</p>
        <p className="mt-3 font-semibold text-text-primary">What this task involves</p>
        <ul className="mt-1.5 list-disc space-y-1 pl-4 text-text-secondary">{activity.involves.map(step => <li key={step}>{step}</li>)}</ul>
        <p className="mt-3 leading-5 text-text-muted">These are the task’s steps, not live completion indicators. Live stage updates are not available for this task.</p>
        <p className="mt-2 text-text-muted">Started {new Date(activity.startedAt).toLocaleTimeString()}{activity.finishedAt ? ` · Finished ${new Date(activity.finishedAt).toLocaleTimeString()}` : ''}</p>
      </div>
    </details>
    {pending && now - activity.startedAt >= 60_000 && <p className="mt-1.5 text-xs leading-5 text-text-muted">Still waiting for a response. You can keep browsing Study.</p>}
  </article>;
}

/** Mounted above all Studio screens, so a tab change cannot hide a pending request. */
export function StudyActivityPanel({ onOpen }: { onOpen: (destination: StudyDestination) => void }) {
  // Use the same client as the IPC bridge, including when Studio is remounted.
  const { data: activities = EMPTY_STUDY_ACTIVITY } = useQuery<StudyActivity[]>({
    queryKey: STUDY_ACTIVITY_KEY, enabled: false, initialData: EMPTY_STUDY_ACTIVITY,
    staleTime: Infinity, gcTime: Infinity,
  }, queryClient);
  const [expanded, setExpanded] = useState(true);
  const [now, setNow] = useState(Date.now);
  const seen = useRef(new Set<string>());
  const seenFailures = useRef(new Set<string>());
  const activityList = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const active = activities.filter(item => item.status === 'pending');
  const failures = activities.filter(item => item.status === 'failed');
  const ordered = [...activities].sort((a, b) => Number(b.status === 'pending') - Number(a.status === 'pending') || b.startedAt - a.startedAt);

  useEffect(() => {
    if (activities.some(item => !seen.current.has(item.id) || (item.status === 'failed' && !seenFailures.current.has(item.id)))) {
      setExpanded(true);
      if (activityList.current) activityList.current.scrollTop = 0;
    }
    seen.current = new Set(activities.map(item => item.id));
    seenFailures.current = new Set(activities.filter(item => item.status === 'failed').map(item => item.id));
  }, [activities]);
  useEffect(() => {
    if (!active.length) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active.length]);

  if (!activities.length) return null;
  return <section aria-label="Study activity" className="shrink-0 border-b border-border-default bg-surface px-4 py-3 shadow-xs sm:px-6">
    <div className="mx-auto max-w-[1500px]">
      <div className="flex items-center justify-between gap-3">
        <button type="button" aria-expanded={expanded} aria-controls={panelId} onClick={() => setExpanded(value => !value)} className="flex min-h-10 min-w-0 flex-1 items-center gap-3 rounded-lg text-left focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">
          <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg border border-border-subtle bg-surface-sunken text-accent"><Clock3 size={16} aria-hidden="true" /></span>
          <span className="min-w-0 flex-1"><span className="block text-sm font-semibold text-text-primary">Study activity</span><span role="status" className="block text-xs text-text-muted">{active.length ? `${active.length} running` : 'No requests running'}{failures.length ? ` · ${failures.length} ${failures.length === 1 ? 'needs' : 'need'} attention` : ''}</span></span>
          {!expanded && active.length > 0 && <span aria-live="off" className="text-xs tabular-nums text-text-muted">{duration(now - Math.min(...active.map(item => item.startedAt)))}</span>}
          {expanded ? <ChevronUp size={16} aria-hidden="true" className="text-text-muted" /> : <ChevronDown size={16} aria-hidden="true" className="text-text-muted" />}
          <span className="sr-only">{expanded ? 'Collapse activity' : 'Expand activity'}</span>
        </button>
        {activities.some(item => item.status !== 'pending') && <button type="button" onClick={() => dismissStudyActivity()} aria-label="Clear finished activity" title="Clear finished activity" className="grid h-9 w-9 shrink-0 place-items-center rounded-lg text-text-muted transition hover:bg-surface-raised hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"><X size={16} /></button>}
      </div>
      <div id={panelId} hidden={!expanded}>
        <p className="my-2.5 text-xs leading-relaxed text-text-muted">{active.length ? 'Work continues while you browse Study. Collapse this panel to keep reading.' : 'Recent Study requests. Open a result or clear finished activity.'}</p>
        <div ref={activityList} className="grid max-h-[38vh] gap-2.5 overflow-y-auto pb-1 pr-1">{ordered.map(activity => <ActivityCard key={activity.id} activity={activity} now={now} onOpen={destination => { setExpanded(false); onOpen(destination); }} />)}</div>
      </div>
    </div>
  </section>;
}
