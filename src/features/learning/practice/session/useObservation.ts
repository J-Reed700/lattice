import { useCallback, useEffect, useRef } from 'react';

import type * as Wire from '@/lib/bindings';

import type { KindDraft } from './kind';
import { sessionApi } from './sessionApi';

/** How long typing must stop before a Watched interviewer looks at the changes. */
export const TYPING_PAUSE_MS = 4_000;
/** How long a Coached learner may do nothing before the coach checks in. */
export const IDLE_AFTER_MS = 180_000;

interface ObservationArgs {
  programId: string;
  session: Wire.PracticeSessionDto;
  draft: KindDraft;
  /** Changes whenever the learner does something. */
  activityAt: number;
  apply: (next: Wire.PracticeSessionDto) => void;
}

/** The pause and idle observations the counterpart looks at the work on. */
export function useObservation({ programId, session, draft, activityAt, apply }: ObservationArgs) {
  const { sessionId, setting, finished } = session;
  const draftRef = useRef(draft);
  draftRef.current = draft;

  const observe = useCallback(
    async (reason: Wire.DraftObservation) => {
      try {
        apply(await sessionApi.observeDraft({ programId, sessionId, piece: draftRef.current.piece(), reason }));
      } catch {
        /* the next checkpoint looks again */
      }
    },
    [apply, programId, sessionId],
  );

  const signature = draft.signature;
  const lastObserved = useRef(signature);
  useEffect(() => {
    if (setting !== 'watched' || finished || signature === lastObserved.current) return;
    const timer = setTimeout(() => {
      lastObserved.current = signature;
      void observe('pause');
    }, TYPING_PAUSE_MS);
    return () => clearTimeout(timer);
  }, [signature, setting, finished, observe]);

  useEffect(() => {
    if (setting !== 'coached' || finished) return;
    const timer = setTimeout(() => void observe('idle'), IDLE_AFTER_MS);
    return () => clearTimeout(timer);
  }, [activityAt, setting, finished, observe]);
}
