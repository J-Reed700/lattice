import { useCallback, useEffect, useRef, useState } from 'react';

import { newId } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';

import { sessionApi } from './sessionApi';
import { useObservation } from './useObservation';
import { useSessionClock } from './useSessionClock';

import type { Failure, KindDraft } from './kind';
import type { StatusState } from '../shared/StatusLine';

interface UseSessionArgs {
  programId: string;
  initial: Wire.PracticeSessionDto;
  draft: KindDraft;
}

/** The session's state and every call the screen makes on the engine. */
export function useSession({ programId, initial, draft }: UseSessionArgs) {
  const sessionId = initial.sessionId;
  const [session, setSession] = useState(initial);
  const sessionRef = useRef(initial);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const movingRef = useRef(false);
  const checkingRef = useRef(false);
  const [activityAt, setActivityAt] = useState(0);
  const [running, setRunning] = useState(0);
  const [asking, setAsking] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [moving, setMoving] = useState(false);
  const [replyBusy, setReplyBusy] = useState(false);
  const [replyError, setReplyError] = useState<string | null>(null);
  const [sendError, setSendError] = useState<Failure | null>(null);
  const [actionError, setActionError] = useState<Failure | null>(null);

  const alive = useRef(true);
  const responding = useRef(false);
  const drainRef = useRef<() => Promise<void>>(async () => undefined);

  const apply = useCallback((next: Wire.PracticeSessionDto) => {
    sessionRef.current = next;
    setSession(next);
    if (next.cuePending) void drainRef.current();
  }, []);

  const drain = useCallback(async () => {
    if (responding.current) return;
    responding.current = true;
    setReplyBusy(true);
    setReplyError(null);
    try {
      while (alive.current && sessionRef.current.cuePending) {
        apply(await sessionApi.respond({ programId, sessionId }));
      }
    } catch (error) {
      if (alive.current) setReplyError(getErrorMessage(error));
    } finally {
      responding.current = false;
      if (alive.current) setReplyBusy(false);
    }
  }, [apply, programId, sessionId]);
  drainRef.current = drain;

  useEffect(() => {
    alive.current = true;
    if (sessionRef.current.cuePending) void drainRef.current();
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      apply(await sessionApi.getSession({ programId, sessionId }));
    } catch {
      /* the failure that led here is already shown */
    }
  }, [apply, programId, sessionId]);

  const touch = useCallback(() => setActivityAt((value) => value + 1), []);

  const { clock, elapsedMs } = useSessionClock(programId, initial, sessionRef);
  useObservation({ programId, session, draft, activityAt, apply });

  const guarded = useCallback(
    async (work: () => Promise<Wire.PracticeSessionDto>, retry?: () => void) => {
      setActionError(null);
      try {
        apply(await work());
        return true;
      } catch (error) {
        setActionError({ message: getErrorMessage(error), retry });
        void refresh();
        return false;
      }
    },
    [apply, refresh],
  );

  const check = useCallback(
    async (
      scope: 'shown' | 'all',
      ids: { operationId: string; checkId: string } = { operationId: newId(), checkId: newId() },
    ) => {
      if (checkingRef.current) return;
      checkingRef.current = true;
      touch();
      setRunning((count) => count + 1);
      try {
        await guarded(
          () => sessionApi.check({ ...ids, programId, sessionId, scope, piece: draftRef.current.piece() }),
          () => void check(scope, ids),
        );
      } finally {
        checkingRef.current = false;
        setRunning((count) => count - 1);
      }
    },
    [guarded, programId, sessionId, touch],
  );

  const own = useCallback(
    async (call: () => Promise<Wire.PracticeSessionDto>, retry?: () => void) => {
      setRunning((count) => count + 1);
      try {
        return await guarded(call, retry);
      } finally {
        setRunning((count) => count - 1);
      }
    },
    [guarded],
  );

  const ask = async (operationId = newId()) => {
    setAsking(true);
    await guarded(
      () =>
        sessionApi.askHint({
          operationId,
          programId,
          sessionId,
          piece: draftRef.current.piece(),
          expectedRevision: sessionRef.current.revision,
        }),
      () => void ask(operationId),
    );
    setAsking(false);
  };

  const move = async (stage: Wire.Stage) => {
    if (movingRef.current) return;
    movingRef.current = true;
    setMoving(true);
    try {
      await guarded(() =>
        sessionApi.moveStage({
          operationId: newId(),
          programId,
          sessionId,
          stage,
          expectedRevision: sessionRef.current.revision,
        }),
      );
    } finally {
      movingRef.current = false;
      setMoving(false);
    }
  };

  const changeSetting = (setting: Wire.Setting) =>
    guarded(() =>
      sessionApi.setSetting({
        operationId: newId(),
        programId,
        sessionId,
        setting,
        expectedRevision: sessionRef.current.revision,
      }),
    );

  const reveal = () => guarded(() => sessionApi.revealSolution({ operationId: newId(), programId, sessionId }));

  const send = async (content: string, spoken: boolean, operationId = newId()): Promise<void> => {
    touch();
    setSendError(null);
    try {
      apply(
        await sessionApi.sendTurn({
          operationId,
          programId,
          sessionId,
          content,
          spoken,
          piece: draftRef.current.piece(),
          expectedRevision: sessionRef.current.revision,
        }),
      );
    } catch (error) {
      setSendError({ message: getErrorMessage(error), retry: () => void send(content, spoken, operationId) });
      void refresh();
    }
  };

  const finish = async (operationId = newId()) => {
    setFinishing(true);
    await draftRef.current.flush();
    await guarded(
      () => sessionApi.finish({ operationId, programId, sessionId, piece: draftRef.current.piece() }),
      () => void finish(operationId),
    );
    setFinishing(false);
  };

  const shown = actionError ?? draft.failure;
  const status: StatusState | null = finishing
    ? { kind: 'busy', label: 'Writing your debrief…' }
    : shown
      ? { kind: 'error', message: shown.message, retry: shown.retry }
      : null;

  return {
    session,
    clock,
    elapsedMs,
    status,
    checking: running > 0,
    asking,
    finishing,
    moving,
    replyBusy,
    threadError: replyError ? { message: replyError, retry: () => void drain() } : sendError,
    touch,
    check,
    own,
    ask,
    move,
    changeSetting,
    reveal,
    send,
    finish,
  };
}
