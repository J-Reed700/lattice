import { useEffect, useState } from 'react';

import type * as Wire from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';

import { PracticeWorkspace } from './PracticeWorkspace';
import { sessionApi } from './sessionApi';
import { StatusLine, type StatusState } from '../shared/StatusLine';

import type { Kinds, KindView } from './kind';

export interface PracticeSessionProps {
  programId: string;
  sessionId: string;
  kinds: Kinds;
  onExit: () => void;
  onTryInterview: (setting: Wire.Setting) => void;
}

type Loaded = { session: Wire.PracticeSessionDto; kind: KindView; prepared: unknown };
type Failed = { message: string; retryable: boolean };

export function PracticeSession({ programId, sessionId, kinds, onExit, onTryInterview }: PracticeSessionProps) {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [failure, setFailure] = useState<Failed | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setFailure(null);
    void (async () => {
      try {
        const session = await sessionApi.getSession({ programId, sessionId });
        const kind = kinds[session.kind] as KindView | undefined;
        if (!kind) {
          if (!cancelled) {
            setFailure({ message: 'This kind of practice is not available in this version of the app.', retryable: false });
          }
          return;
        }
        const prepared = await kind.prepare(programId, session);
        if (!cancelled) setLoaded({ session, kind, prepared });
      } catch (error) {
        if (!cancelled) setFailure({ message: getErrorMessage(error), retryable: true });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [programId, sessionId, kinds, attempt]);

  if (!loaded) {
    const state: StatusState = failure
      ? {
          kind: 'error',
          message: failure.message,
          retry: failure.retryable ? () => setAttempt((value) => value + 1) : undefined,
        }
      : { kind: 'busy', label: 'Opening your session…' };
    return (
      <div className="p-4">
        <StatusLine state={state} />
        {failure ? (
          <button
            type="button"
            onClick={onExit}
            className="mt-3 min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
          >
            Back to Practice
          </button>
        ) : null}
      </div>
    );
  }

  return (
    <PracticeWorkspace
      programId={programId}
      initial={loaded.session}
      prepared={loaded.prepared}
      kind={loaded.kind}
      onExit={onExit}
      onTryInterview={onTryInterview}
    />
  );
}
