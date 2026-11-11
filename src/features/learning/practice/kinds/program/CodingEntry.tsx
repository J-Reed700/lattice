import { useCallback, useEffect, useRef, useState } from 'react';

import { AlertCircle, Check, Circle, Loader2 } from 'lucide-react';

import { useJobStatus } from '@/features/jobs/api';
import { newId } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';

import { programApi } from './programApi';
import type { KindEntryProps } from '../../session/kind';
import { SETTING_NAMES } from '../../session/practiceNames';
import { sessionApi } from '../../session/sessionApi';

const STEPS = ['Writing the problem', 'Proving it', 'Saving'] as const;

type Phase = { kind: 'building' } | { kind: 'starting' } | { kind: 'failed'; message: string; stage: 'build' | 'start' };

export function CodingEntry({ programId, choice, onReady, onCancel }: KindEntryProps) {
  const [phase, setPhase] = useState<Phase>({ kind: 'building' });
  const [step, setStep] = useState(0);
  const [attempt, setAttempt] = useState(0);
  const jobId = useRef<string | null>(null);
  const seen = useRef(new Map<string, Wire.JobDto>());
  const operationId = useRef(newId());
  const handled = useRef<string | null>(null);
  const activityId = useRef<string | null>(null);
  const startOperationId = useRef(newId());
  const latest = useRef({ choice, onReady });
  latest.current = { choice, onReady };

  const start = useCallback(
    async (activity: string) => {
      setPhase({ kind: 'starting' });
      try {
        const { choice: picked, onReady: ready } = latest.current;
        const session = await sessionApi.startSession({
          operationId: startOperationId.current,
          programId,
          activityId: activity,
          setting: picked.setting,
          budgetSeconds: picked.budgetSeconds ?? null,
        });
        ready(session.sessionId);
      } catch (error) {
        setPhase({ kind: 'failed', message: getErrorMessage(error), stage: 'start' });
      }
    },
    [programId],
  );

  const settle = useCallback(
    async (job: Wire.JobDto) => {
      if (job.id !== jobId.current) return;
      setStep(job.progressCurrent);
      if (job.status === 'completed' && job.resultRef && handled.current !== job.id) {
        handled.current = job.id;
        activityId.current = job.resultRef;
        await start(job.resultRef);
      } else if (job.status === 'failed' || job.status === 'cancelled' || job.status === 'interrupted') {
        setPhase({ kind: 'failed', message: job.error ?? 'The problem could not be built.', stage: 'build' });
      }
    },
    [start],
  );

  useJobStatus((job) => {
    seen.current.set(job.id, job);
    void settle(job);
  });

  useEffect(() => {
    let cancelled = false;
    const { choice: picked } = latest.current;
    programApi
      .startBuild({
        operationId: operationId.current,
        programId,
        lessonId: picked.lessonId ?? null,
        form: picked.form,
        language: picked.language,
        difficulty: picked.difficulty,
        focus: picked.focus ?? null,
      })
      .then((result) => {
        if (cancelled) return;
        jobId.current = result.jobId;
        const early = seen.current.get(result.jobId);
        if (early) void settle(early);
      })
      .catch((error: unknown) => {
        if (!cancelled) setPhase({ kind: 'failed', message: getErrorMessage(error), stage: 'build' });
      });
    return () => {
      cancelled = true;
    };
  }, [programId, attempt, settle]);

  const retry = () => {
    if (phase.kind === 'failed' && phase.stage === 'start' && activityId.current) {
      void start(activityId.current);
      return;
    }
    operationId.current = newId();
    startOperationId.current = newId();
    activityId.current = null;
    jobId.current = null;
    handled.current = null;
    seen.current.clear();
    setStep(0);
    setPhase({ kind: 'building' });
    setAttempt((value) => value + 1);
  };

  const failed = phase.kind === 'failed';
  const current = Math.min(step, STEPS.length);

  return (
    <main className="mx-auto flex w-full max-w-2xl flex-col gap-5 px-4 py-10">
      <header className="flex flex-col gap-1">
        <p className="m-0 text-xs font-semibold uppercase tracking-wide text-text-secondary">
          Code · {SETTING_NAMES[choice.setting]}
        </p>
        <h1 className="m-0 font-serif text-2xl font-bold text-text-primary">Building your problem</h1>
        <p className="m-0 text-sm text-text-secondary">
          It is checked before you see it, so the expected answers are right. You can leave: it keeps building.
        </p>
      </header>

      <div
        role="progressbar"
        aria-label="Building your problem"
        aria-valuemin={0}
        aria-valuemax={STEPS.length}
        aria-valuenow={current}
        aria-valuetext={`Step ${Math.min(current + 1, STEPS.length)} of ${STEPS.length}`}
        className="h-2 w-full overflow-hidden rounded-full bg-surface-raised"
      >
        <div className="h-full rounded-full bg-accent transition-all" style={{ width: `${(current / STEPS.length) * 100}%` }} />
      </div>

      <ol className="m-0 flex list-none flex-col gap-3 p-0">
        {STEPS.map((name, index) => {
          const done = index < current;
          const active = index === current && !failed;
          return (
            <li key={name} className="flex items-center gap-3 text-sm text-text-primary">
              {done ? (
                <Check className="h-5 w-5 text-success-fg" aria-hidden="true" />
              ) : active ? (
                <Loader2 className="h-5 w-5 animate-spin text-accent" aria-hidden="true" />
              ) : (
                <Circle className="h-5 w-5 text-text-secondary" aria-hidden="true" />
              )}
              <span>{name}</span>
              <span className="text-text-secondary">{done ? 'Done' : active ? 'Working' : 'Waiting'}</span>
            </li>
          );
        })}
      </ol>

      {phase.kind === 'starting' ? (
        <p role="status" className="m-0 text-sm text-text-secondary">
          Opening your session…
        </p>
      ) : null}
      {phase.kind === 'failed' ? (
        <div role="alert" className="flex flex-wrap items-center gap-3 text-sm text-danger-fg">
          <AlertCircle className="h-4 w-4" aria-hidden="true" />
          <span>{phase.message}</span>
          <button
            type="button"
            onClick={retry}
            className="min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
          >
            Try again
          </button>
        </div>
      ) : null}

      <div>
        <button
          type="button"
          onClick={onCancel}
          className="min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
        >
          Cancel
        </button>
      </div>
    </main>
  );
}
