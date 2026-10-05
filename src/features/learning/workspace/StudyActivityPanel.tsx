import { useEffect, useId, useRef, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { Check, ChevronDown, ChevronUp, Clock3, LoaderCircle, X } from 'lucide-react';

import { queryClient } from '@/lib/queryClient';
import { dismissStudyActivity, EMPTY_STUDY_ACTIVITY, STUDY_ACTIVITY_KEY, type StudyActivity, type StudyDestination } from '@/lib/studyActivity';

function duration(milliseconds: number) {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

function ActivityCard({ activity, now, onOpen }: { activity: StudyActivity; now: number; onOpen: (destination: StudyDestination) => void }) {
  const pending = activity.status === 'pending';
  const failed = activity.status === 'failed';
  const destination = activity.destination;
  return <article aria-label={activity.title} className="rounded-xl border border-border bg-surface p-4">
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0">
        {activity.courseTitle && <p className="mb-1 truncate text-xs text-text-muted">{activity.courseTitle}</p>}
        <h3 className="text-sm font-semibold text-text-primary">{activity.title}</h3>
        <p role="status" className={`mt-1 flex items-center gap-1.5 text-xs ${failed ? 'text-rose-700 dark:text-rose-300' : 'text-text-secondary'}`}>
          {pending ? <LoaderCircle size={13} className="shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" /> : !failed ? <Check size={13} aria-hidden="true" /> : null}
          {pending ? 'Waiting for the result' : failed ? 'Request failed' : 'Request completed'}
        </p>
      </div>
      <span className="shrink-0 text-xs tabular-nums text-text-muted" aria-live="off">{pending ? 'Elapsed' : 'Took'} {duration((activity.finishedAt ?? now) - activity.startedAt)}</span>
    </div>
    <p role={failed ? 'alert' : undefined} className="mt-3 text-sm leading-6 text-text-secondary">{pending ? activity.detail : failed ? activity.error : 'The app has returned a result. Open the task to review it.'}</p>
    <details key={activity.status} className="mt-3 text-xs text-text-secondary">
      <summary className="cursor-pointer rounded-md py-1 font-medium text-accent focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent">Task details</summary>
      <div className="mt-2 rounded-lg bg-background p-3">
        <p className="font-medium">What this task involves</p>
        <ul className="mt-2 list-disc space-y-1 pl-4">{activity.involves.map(step => <li key={step}>{step}</li>)}</ul>
        <p className="mt-3 leading-5 text-text-muted">These are the task’s steps, not live completion indicators. Live stage updates are not available for this task.</p>
        <p className="mt-2 text-text-muted">Started {new Date(activity.startedAt).toLocaleTimeString()}{activity.finishedAt ? ` · Finished ${new Date(activity.finishedAt).toLocaleTimeString()}` : ''}</p>
      </div>
    </details>
    {pending && now - activity.startedAt >= 60_000 && <p className="mt-3 text-xs leading-5 text-text-muted">Still waiting for a response. Live stage updates are not available for this task. You can keep browsing Study.</p>}
    <div className="mt-3 flex flex-wrap items-center gap-2">
      {(destination.programId || destination.flashcards) && <button type="button" onClick={() => onOpen(destination)} className="min-h-9 rounded-full border border-border px-3 text-xs font-medium text-text-primary hover:border-accent focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent">{failed ? 'Return to task to retry' : pending ? 'Open task' : 'View result'}</button>}
      {!pending && <button type="button" onClick={() => dismissStudyActivity(activity.id)} className="min-h-9 rounded-full px-3 text-xs text-text-muted hover:bg-background focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent">Dismiss</button>}
    </div>
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
  return <section aria-label="Study activity" className="shrink-0 border-b border-accent/20 bg-surface px-4 py-3 sm:px-6">
    <div className="mx-auto max-w-[1500px]">
      <div className="flex items-center justify-between gap-3">
        <button type="button" aria-expanded={expanded} aria-controls={panelId} onClick={() => setExpanded(value => !value)} className="flex min-h-10 min-w-0 flex-1 items-center gap-3 rounded-lg text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent">
          <span className="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-accent/10 text-accent"><Clock3 size={16} aria-hidden="true" /></span>
          <span className="min-w-0 flex-1"><span className="block text-sm font-semibold text-text-primary">Study activity</span><span role="status" className="block text-xs text-text-muted">{active.length ? `${active.length} running` : 'No requests running'}{failures.length ? ` · ${failures.length} ${failures.length === 1 ? 'needs' : 'need'} attention` : ''}</span></span>
          {!expanded && active.length > 0 && <span aria-live="off" className="text-xs tabular-nums text-text-muted">{duration(now - Math.min(...active.map(item => item.startedAt)))}</span>}
          {expanded ? <ChevronUp size={16} aria-hidden="true" /> : <ChevronDown size={16} aria-hidden="true" />}
          <span className="sr-only">{expanded ? 'Collapse activity' : 'Expand activity'}</span>
        </button>
        {activities.some(item => item.status !== 'pending') && <button type="button" onClick={() => dismissStudyActivity()} aria-label="Clear finished activity" title="Clear finished activity" className="grid h-9 w-9 shrink-0 place-items-center rounded-full text-text-muted hover:bg-background focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"><X size={16} /></button>}
      </div>
      <div id={panelId} hidden={!expanded}>
        <p className="my-3 text-xs leading-5 text-text-muted">{active.length ? 'Work continues while you browse Study. Collapse this panel to keep reading.' : 'Recent Study requests. Open a result or clear finished activity.'}</p>
        <div ref={activityList} className={`grid max-h-[38vh] gap-3 overflow-y-auto pb-1 pr-1 ${ordered.length > 1 ? 'lg:grid-cols-2' : ''}`}>{ordered.map(activity => <ActivityCard key={activity.id} activity={activity} now={now} onOpen={destination => { setExpanded(false); onOpen(destination); }} />)}</div>
      </div>
    </div>
  </section>;
}
