import { useCallback, useEffect, useRef } from 'react';

import { AlertCircle, Check, Circle, Loader2 } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

import { useCodingBuildStore } from './codingBuildStore';
import { SETTING_NAMES } from '../../session/practiceNames';

export interface CodingBuildChoice {
  lessonId?: string | null;
  form: Wire.Form;
  setting: Wire.Setting;
  language: string;
  difficulty: Wire.ProblemDifficulty;
  focus?: string | null;
  budgetSeconds?: number | null;
}

export interface CodingEntryProps {
  programId: string;
  choice: CodingBuildChoice;
  onReady: (sessionId: string) => void;
  onCancel: () => void;
}

const STEPS = ['Writing the problem', 'Proving it', 'Saving'] as const;

export function CodingEntry({ programId, choice, onReady, onCancel }: CodingEntryProps) {
  const handle = useCodingBuildStore((store) => store.handles[programId]);
  const ensureBuild = useCodingBuildStore((store) => store.ensureBuild);
  const startSessionForBuild = useCodingBuildStore((store) => store.startSessionForBuild);
  const clearHandle = useCodingBuildStore((store) => store.clearHandle);

  const mountedRef = useRef(true);
  const latest = useRef({ choice, onReady });
  latest.current = { choice, onReady };

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    void ensureBuild(programId, latest.current.choice);
  }, [ensureBuild, programId]);

  const launchSession = useCallback(async () => {
    await startSessionForBuild(programId);
  }, [programId, startSessionForBuild]);

  const currentPhase = handle?.phase;
  const phaseKind = currentPhase?.kind;
  const readySessionId = currentPhase?.kind === 'ready' ? currentPhase.sessionId : null;
  const activityId = handle?.activityId ?? null;
  useEffect(() => {
    if (phaseKind === 'ready' && readySessionId) {
      clearHandle(programId);
      latest.current.onReady(readySessionId);
    }
  }, [clearHandle, phaseKind, programId, readySessionId]);
  useEffect(() => {
    if (phaseKind === 'built' && activityId) {
      void launchSession();
    }
  }, [activityId, launchSession, phaseKind]);

  const retry = () => {
    if (handle?.phase.kind === 'failed' && handle.phase.stage === 'start' && handle.activityId) {
      void launchSession();
      return;
    }
    void ensureBuild(programId, handle?.choice ?? latest.current.choice, true);
  };

  const activeChoice = handle?.choice ?? choice;
  const phase = handle?.phase ?? { kind: 'building' as const };
  const step = handle?.step ?? 0;
  const failed = phase.kind === 'failed';
  const current = Math.min(step, STEPS.length);

  return (
    <main className="mx-auto flex w-full max-w-2xl flex-col gap-5 px-4 py-10">
      <header className="flex flex-col gap-1">
        <p className="m-0 text-xs font-semibold uppercase tracking-wide text-text-secondary">
          Code · {SETTING_NAMES[activeChoice.setting]}
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

      {phase.kind === 'starting' || phase.kind === 'built' ? (
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
          className="min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-secondary hover:bg-surface-raised hover:text-text-primary"
        >
          Cancel
        </button>
      </div>
    </main>
  );
}
