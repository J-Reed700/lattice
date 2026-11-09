import { useEffect, useState } from 'react';
import { ArrowRight, Check, Loader2 } from 'lucide-react';
import { useJobStatus, type JobDto } from '@/features/jobs/api';
import {
  codingApi,
  type CodingSessionDto,
  type CodingTask,
  type Difficulty,
  type Setting,
} from './codingApi';
import { CodingSession } from './CodingSession';
import { Debrief } from './Debrief';

export interface CodingEntryRequest {
  task: CodingTask;
  setting: Setting;
  language: string;
  difficulty: Difficulty;
  focus: string | null;
  budgetMinutes: number | null;
}

export interface CodingEntryProps {
  programId: string;
  lessonId: string | null;
  request: CodingEntryRequest;
  onExit: () => void;
  sessionId?: string | null;
}

type EntryView = 'building' | 'session' | 'debrief';

function extractActivityId(job: Partial<JobDto>): string | null {
  if (job.activity && typeof job.activity === 'object' && 'activityId' in job.activity) {
    const act = (job.activity as { activityId?: string }).activityId;
    if (act) return act;
  }
  if (job.resultRef) {
    try {
      const parsed = JSON.parse(job.resultRef);
      if (parsed && typeof parsed === 'object' && 'activityId' in parsed) {
        return parsed.activityId;
      }
    } catch {
      // Not JSON, assume raw string id
    }
    return job.resultRef;
  }
  return null;
}

export function CodingEntry({
  programId,
  lessonId,
  request,
  onExit,
  sessionId,
}: CodingEntryProps) {
  const [view, setView] = useState<EntryView>('building');
  const [activeSession, setActiveSession] = useState<CodingSessionDto | null>(null);

  // Build state
  const [buildJobId, setBuildJobId] = useState<string | null>(null);
  const [buildReady, setBuildReady] = useState(false);
  const [activityId, setActivityId] = useState<string | null>(null);
  const [stepCount, setStepCount] = useState(2); // Starts at step 2 of 3 (drafting done, proving underway)

  // 1. If sessionId was passed, resume existing session directly
  useEffect(() => {
    if (sessionId) {
      void codingApi.getCodingSession(sessionId).then((session) => {
        setActiveSession(session);
        if (session.finishedAt && session.debrief) {
          setView('debrief');
        } else {
          setView('session');
        }
      });
      return;
    }

    // 2. Otherwise start a new problem build
    let cancelled = false;
    void codingApi
      .startCodingBuild({
        programId,
        lessonId,
        task: request.task,
        setting: request.setting,
        language: request.language,
        difficulty: request.difficulty,
        focus: request.focus,
      })
      .then((res) => {
        if (!cancelled && res?.jobId) {
          setBuildJobId(res.jobId);
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [sessionId, programId, lessonId, request]);

  // Listen for job status changes
  useJobStatus((job) => {
    if (buildJobId && job.id !== buildJobId) return;

    if (job.progressCurrent !== undefined) {
      setStepCount(job.progressCurrent);
    }

    if (job.status === 'completed') {
      const resolvedActivityId = extractActivityId(job);
      if (resolvedActivityId) {
        setActivityId(resolvedActivityId);
      }
      setBuildReady(true);
      setStepCount(3);
    }
  });

  const handleStartSession = async () => {
    const actId = activityId || 'activity-1';
    const newSessionId = `session-${Date.now()}`;
    const opId = `start-${Date.now()}`;
    const budgetSeconds = request.budgetMinutes ? request.budgetMinutes * 60 : null;

    try {
      const session = await codingApi.startCodingSession({
        operationId: opId,
        sessionId: newSessionId,
        programId,
        activityId: actId,
        setting: request.setting,
        budgetSeconds,
      });
      setActiveSession(session);
      setView('session');
    } catch {
      // Fallback or retry
    }
  };

  const handleSessionFinished = (finished: CodingSessionDto) => {
    setActiveSession(finished);
    if (finished.debrief) {
      setView('debrief');
    }
  };

  // Render active session
  if (view === 'session' && activeSession) {
    return (
      <CodingSession
        session={activeSession}
        onUpdateSession={setActiveSession}
        onFinish={handleSessionFinished}
        onExit={onExit}
      />
    );
  }

  // Render debrief
  if (view === 'debrief' && activeSession?.debrief) {
    return (
      <Debrief
        debrief={activeSession.debrief}
        programId={programId}
        lessonId={lessonId}
        onExit={onExit}
        onRetryInterview={() => {
          // Restart under interview conditions
          setView('building');
          setBuildReady(false);
          setStepCount(2);
        }}
      />
    );
  }

  // Render Building problem view (Building.dc.html)
  const settingLabel =
    request.setting === 'coached'
      ? 'Coached'
      : request.setting === 'mock'
      ? 'Mock'
      : 'Watched';

  const headline = buildReady ? 'Your problem is ready' : 'Building your problem';
  const progressText = buildReady ? 'All 3 steps done' : `Step ${stepCount} of 3: proving it`;
  const pct = buildReady ? 100 : Math.round((stepCount / 3) * 100);

  return (
    <div className="mx-auto max-w-2xl px-4 py-12 sm:px-6">
      <div className="flex flex-col gap-6 rounded-2xl border border-border-default bg-surface p-6 sm:p-8 shadow-xs">
        {/* Section title & Subhead */}
        <section aria-labelledby="building-title" className="flex flex-col gap-2">
          <div className="text-xxs font-semibold uppercase tracking-wider text-text-tertiary">
            Code · {settingLabel}
          </div>
          <h1
            id="building-title"
            className="font-serif text-2xl font-bold tracking-tight text-text-primary sm:text-3xl m-0"
          >
            {headline}
          </h1>
          <p className="text-sm text-text-secondary m-0">
            An easy Python problem, coached. It&apos;s checked before you see it, so the
            expected answers are right.
          </p>
        </section>

        {/* Progress bar */}
        <div
          role="progressbar"
          aria-label="Building your problem"
          aria-valuemin={0}
          aria-valuemax={3}
          aria-valuenow={stepCount}
          aria-valuetext={progressText}
          className="h-2 w-full overflow-hidden rounded-full bg-surface-raised"
        >
          <div
            className="h-full rounded-full bg-accent transition-all duration-300"
            style={{ width: `${pct}%` }}
          />
        </div>

        {/* 3 Step Checklist */}
        <ol className="flex flex-col gap-4 list-none m-0 p-0">
          {/* Step 1 */}
          <li className="flex items-start gap-3.5">
            <span className="grid h-7 w-7 flex-none place-items-center rounded-full bg-success-muted text-success-fg">
              <Check className="h-4 w-4" aria-hidden="true" />
            </span>
            <div className="flex-1 min-w-0">
              <div className="font-semibold text-sm text-text-primary">
                Picked what to practise
              </div>
              <div className="text-xs text-text-secondary">
                Two patterns: one due today, one you haven&apos;t tried. Their names stay hidden
                until your debrief.
              </div>
            </div>
            <span className="flex-none font-mono text-xs text-text-tertiary pt-0.5">
              1 s
            </span>
          </li>

          {/* Step 2 */}
          <li className="flex items-start gap-3.5">
            <span className="grid h-7 w-7 flex-none place-items-center rounded-full bg-success-muted text-success-fg">
              <Check className="h-4 w-4" aria-hidden="true" />
            </span>
            <div className="flex-1 min-w-0">
              <div className="font-semibold text-sm text-text-primary">
                Wrote the problem
              </div>
              <div className="text-xs text-text-secondary">
                A statement, 8 cases and 3 common mistakes.
              </div>
            </div>
            <span className="flex-none font-mono text-xs text-text-tertiary pt-0.5">
              11 s
            </span>
          </li>

          {/* Step 3 */}
          <li className="flex items-start gap-3.5">
            {buildReady ? (
              <span className="grid h-7 w-7 flex-none place-items-center rounded-full bg-success-muted text-success-fg">
                <Check className="h-4 w-4" aria-hidden="true" />
              </span>
            ) : (
              <span className="grid h-7 w-7 flex-none place-items-center rounded-full bg-accent-muted text-accent">
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              </span>
            )}
            <div className="flex-1 min-w-0">
              <div className="font-semibold text-sm text-text-primary">
                {buildReady ? 'Proved it' : 'Proving it'}
              </div>
              <div className="text-xs text-text-secondary">
                {buildReady
                  ? 'The reference passes every case, and each common mistake fails at least one. Python is warmed up, so your first run is instant.'
                  : 'The reference passes 8 of 8 cases. Checking that each common mistake fails one: 2 of 3.'}
              </div>
            </div>
            {buildReady && (
              <span className="flex-none font-mono text-xs text-text-tertiary pt-0.5">
                6 s
              </span>
            )}
          </li>
        </ol>

        {/* Reassurance text & Back to practice */}
        <div className="flex flex-wrap items-center justify-between gap-3 pt-2 text-xs text-text-secondary border-t border-border-subtle">
          <span>
            You can leave this page. It keeps building, and you&apos;ll find it under Continue.
          </span>
          <button
            type="button"
            onClick={onExit}
            className="cursor-pointer font-medium text-text-secondary hover:text-text-primary underline"
          >
            Back to Practice
          </button>
        </div>

        {/* Action button section */}
        <div className="flex flex-wrap items-center gap-3 pt-2">
          {!buildReady ? (
            <div className="flex flex-col gap-1.5">
              <button
                type="button"
                disabled={true}
                aria-describedby="start-reason"
                className="inline-flex min-h-[44px] cursor-not-allowed items-center justify-center rounded-lg bg-accent px-5 text-sm font-semibold text-accent-fg opacity-50"
              >
                Start
              </button>
              <span id="start-reason" className="text-xs text-text-tertiary">
                Ready when the proof finishes.
              </span>
            </div>
          ) : (
            <button
              type="button"
              onClick={handleStartSession}
              className="inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg bg-accent px-5 text-sm font-semibold text-accent-fg hover:opacity-90"
            >
              <span>Start</span>
              <ArrowRight className="h-4 w-4" aria-hidden="true" />
            </button>
          )}

          <button
            type="button"
            onClick={onExit}
            className="inline-flex min-h-[44px] cursor-pointer items-center justify-center rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
          >
            Cancel build
          </button>
        </div>

        {/* Screen reader live status announcement */}
        <p role="status" className="sr-only">
          {buildReady ? 'Your problem is ready.' : ''}
        </p>
      </div>
    </div>
  );
}
