import { useCallback, useEffect, useRef, useState } from 'react';

import { useSearchParams } from 'react-router';

import { isJobFinished, useJobStatus, type JobDto } from '@/features/jobs/api';
import type { StartedCodeReviewDto } from '@/lib/bindings';

import { drillsApi } from './api';
import { materialOf, readChoices, sizeOf, validMinutes, writeChoices, type DrillOptions } from './drillOptions';
import { initialState, limitOf, reduce, type DrillAction, type DrillState } from './drillState';
import { changedFiles, toFindingDto, type FindingInput } from './findings';
import { clearReviewDraft, EMPTY_DRAFT, readReviewDraft, writeReviewDraft } from './reviewDraft';

/** The address of an attempt: `?drills=open&attempt=<id>`. */
export const ATTEMPT_PARAM = 'attempt';

const DRILL_BUILD = 'learning.drill_build';
const DRILL_GRADING = 'learning.drill_grading';

/** How often a build that is waited for is asked about again, in case the report that ended it was missed. */
const BUILD_POLL_MS = 30_000;
/** How often an attempt that waits for its grade is read again, in case the report that ended its job was missed. */
const GRADING_POLL_MS = 10_000;

/** The shelf of the bank a drill is taken from, which is the subject of the jobs that build for it. */
const shelfOf = (options: DrillOptions) => (options.material === 'passage' ? `passage:${options.programId}` : options.language);

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error));

function buildFailure(job: JobDto): string {
  if (job.error) return job.error;
  return job.status === 'cancelled' ? 'The drill build was cancelled.' : 'The drill could not be built.';
}

/** Calls `tick` every `intervalMs` while `active`. */
function usePoll(tick: () => void, intervalMs: number, active: boolean) {
  const latest = useRef(tick);
  latest.current = tick;
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => latest.current(), intervalMs);
    return () => window.clearInterval(timer);
  }, [active, intervalMs]);
}

/**
 * The code review drill screen's state: the learner's choices, the start and the wait for a build, the review they
 * write, the hand-in and the wait for its grade, and the reveal. The attempt's id is in the address, so an attempt
 * the learner left is read again from the backend when they come back to it.
 */
export function useCodeReviewDrill() {
  const [params, setParams] = useSearchParams();
  const attemptParam = params.get(ATTEMPT_PARAM);
  const [state, setState] = useState<DrillState>(() => initialState(attemptParam ? 'loading' : 'choose', readChoices()));
  // The reducer runs when an action is dispatched, so the state asynchronous work reads is never behind a render.
  const stateRef = useRef(state);
  const attemptRef = useRef<string | null>(null);
  const paramRef = useRef(attemptParam);
  const setParamsRef = useRef(setParams);
  // Each thing the learner does that supersedes the last takes a new run; results of an older run are dropped.
  const runRef = useRef(0);
  const mounted = useRef(false);
  const asking = useRef(false);
  const askAgain = useRef(false);
  paramRef.current = attemptParam;
  setParamsRef.current = setParams;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const dispatch = useCallback((action: DrillAction) => {
    stateRef.current = reduce(stateRef.current, action);
    setState(stateRef.current);
  }, []);

  const live = useCallback((run: number) => mounted.current && run === runRef.current, []);

  /** The attempt the screen holds, and the address that names it. */
  const holdAttempt = useCallback((id: string | null) => {
    attemptRef.current = id;
    if (paramRef.current === id) return;
    setParamsRef.current(
      (current) => {
        const next = new URLSearchParams(current);
        if (id) next.set(ATTEMPT_PARAM, id);
        else next.delete(ATTEMPT_PARAM);
        return next;
      },
      { replace: true },
    );
  }, []);

  const open = useCallback(
    (started: Extract<StartedCodeReviewDto, { status: 'started' }>) => {
      const minutes = started.mode === 'interview' ? stateRef.current.options.minutes : null;
      const draft = { ...EMPTY_DRAFT, minutes };
      writeReviewDraft(started.attemptId, draft);
      holdAttempt(started.attemptId);
      dispatch({
        type: 'opened',
        draft,
        session: {
          attemptId: started.attemptId,
          mode: started.mode,
          startedAt: started.startedAt,
          drill: started.drill,
          limitMs: limitOf(started.mode, minutes),
        },
      });
    },
    [dispatch, holdAttempt],
  );

  const attemptOnce = useCallback(
    async (run: number) => {
      const { options } = stateRef.current;
      const material = materialOf(options);
      if (!material) {
        dispatch({ type: 'failed', failure: { message: 'Choose a course to take a passage from.', retry: 'start' } });
        return;
      }
      const { lines, issues } = sizeOf(options.size, options.material);
      try {
        const started = await drillsApi.startReviewDrill(material, options.mode, lines, issues);
        if (!live(run)) return;
        if (started.status === 'started') open(started);
        else dispatch({ type: 'building', jobId: started.jobId });
      } catch (error) {
        if (live(run)) dispatch({ type: 'failed', failure: { message: messageOf(error), retry: 'start' } });
      }
    },
    [dispatch, live, open],
  );

  /** Asks for a drill. One ask is out at a time, so a drill is never taken twice; asking meanwhile asks once more. */
  const ask = useCallback(async () => {
    if (stateRef.current.stage !== 'starting' || stateRef.current.failure) return;
    if (asking.current) {
      askAgain.current = true;
      return;
    }
    asking.current = true;
    try {
      do {
        askAgain.current = false;
        await attemptOnce(runRef.current);
      } while (askAgain.current && mounted.current && stateRef.current.stage === 'starting' && !stateRef.current.failure);
    } finally {
      asking.current = false;
    }
  }, [attemptOnce]);

  const start = useCallback(
    (patch?: Partial<DrillOptions>) => {
      if (patch) dispatch({ type: 'options', patch });
      const { options } = stateRef.current;
      if (options.mode === 'interview' && !validMinutes(options.minutes)) return;
      if (!materialOf(options)) return;
      runRef.current += 1;
      writeChoices(options);
      // The address keeps naming the last attempt until the new one opens, so it changes once, not twice.
      attemptRef.current = null;
      dispatch({ type: 'starting' });
      void ask();
    },
    [ask, dispatch],
  );

  /** Back to the choices. A build under way goes on without the screen. */
  const reset = useCallback(() => {
    runRef.current += 1;
    holdAttempt(null);
    dispatch({ type: 'reset' });
  }, [dispatch, holdAttempt]);

  const restore = useCallback(
    async (id: string) => {
      attemptRef.current = id;
      runRef.current += 1;
      const run = runRef.current;
      dispatch({ type: 'loading' });
      try {
        const result = await drillsApi.codeReviewResult(id);
        if (!live(run)) return;
        dispatch({ type: 'result', result, draft: result.status === 'open' ? readReviewDraft(id) : EMPTY_DRAFT });
      } catch (error) {
        if (live(run)) dispatch({ type: 'failed', failure: { message: messageOf(error), retry: 'restore' } });
      }
    },
    [dispatch, live],
  );

  const submit = useCallback(async () => {
    const current = stateRef.current;
    if (current.stage !== 'reviewing' || !current.session) return;
    const { attemptId, startedAt, drill } = current.session;
    runRef.current += 1;
    const run = runRef.current;
    dispatch({ type: 'submitting' });
    const fix = changedFiles(drill.files, current.fixEdits);
    try {
      const result = await drillsApi.submitCodeReview(
        attemptId,
        current.findings.map(toFindingDto),
        fix.length > 0 ? fix : null,
        Math.max(0, Math.round(Date.now() - startedAt)),
      );
      if (!live(run)) return;
      clearReviewDraft(attemptId);
      dispatch({ type: 'result', result, draft: EMPTY_DRAFT });
    } catch (error) {
      if (live(run)) dispatch({ type: 'failed', stage: 'reviewing', failure: { message: messageOf(error), retry: 'submit' } });
    }
  }, [dispatch, live]);

  const regrade = useCallback(async () => {
    const id = stateRef.current.result?.attemptId ?? attemptRef.current;
    if (!id || stateRef.current.regrading) return;
    runRef.current += 1;
    const run = runRef.current;
    dispatch({ type: 'regrading' });
    try {
      const result = await drillsApi.regradeCodeReview(id);
      if (!live(run)) return;
      dispatch({ type: 'result', result, draft: EMPTY_DRAFT });
    } catch (error) {
      if (live(run)) dispatch({ type: 'failed', failure: { message: messageOf(error), retry: 'regrade' } });
    }
  }, [dispatch, live]);

  /** Reads the attempt that waits for its grade again. */
  const refresh = useCallback(async () => {
    const id = attemptRef.current;
    if (!id || stateRef.current.stage !== 'grading') return;
    const run = runRef.current;
    try {
      const result = await drillsApi.codeReviewResult(id);
      if (!live(run) || attemptRef.current !== id) return;
      dispatch({ type: 'result', result, draft: EMPTY_DRAFT });
    } catch (error) {
      if (live(run)) dispatch({ type: 'failed', failure: { message: messageOf(error), retry: 'follow' } });
    }
  }, [dispatch, live]);

  // The address leads: an attempt it names is read from the backend; one it no longer names is left.
  useEffect(() => {
    if (attemptParam === attemptRef.current) return;
    if (attemptParam) {
      void restore(attemptParam);
    } else {
      runRef.current += 1;
      attemptRef.current = null;
      dispatch({ type: 'reset' });
    }
  }, [attemptParam, dispatch, restore]);

  useJobStatus((job) => {
    const current = stateRef.current;
    if (current.stage === 'starting' && !current.failure && job.kind === DRILL_BUILD) {
      const ours = current.building?.jobId === job.id;
      if (!ours && job.subjectId !== shelfOf(current.options)) return;
      if (job.status === 'completed') void ask();
      else if (ours && isJobFinished(job)) dispatch({ type: 'failed', failure: { message: buildFailure(job), retry: 'start' } });
      else if (ours) dispatch({ type: 'job', job });
    } else if (current.stage === 'grading' && current.result && job.kind === DRILL_GRADING && job.subjectId === current.result.attemptId) {
      if (isJobFinished(job)) void refresh();
      else dispatch({ type: 'grading-job', message: job.progressMessage });
    }
  });

  const { stage, session, findings, fixEdits } = state;
  usePoll(() => void ask(), BUILD_POLL_MS, stage === 'starting' && state.building !== null && state.failure === null);
  usePoll(() => void refresh(), GRADING_POLL_MS, stage === 'grading' && state.result?.status === 'submitted');

  // What the learner has written waits with the unsent text of the app, so leaving the screen keeps it.
  useEffect(() => {
    if (stage !== 'reviewing' || !session) return;
    writeReviewDraft(session.attemptId, {
      findings,
      fix: fixEdits,
      minutes: session.limitMs === null ? null : session.limitMs / 60_000,
    });
  }, [stage, session, findings, fixEdits]);

  const retry = useCallback(() => {
    const failure = stateRef.current.failure;
    if (!failure) return;
    switch (failure.retry) {
      case 'start':
        start();
        break;
      case 'restore':
        if (attemptRef.current) void restore(attemptRef.current);
        break;
      case 'submit':
        void submit();
        break;
      case 'regrade':
        void regrade();
        break;
      case 'follow':
        void refresh();
        break;
    }
  }, [refresh, regrade, restore, start, submit]);

  /** Another drill on the same material and in the mode of the one just done. */
  const nextDrill = useCallback(() => {
    const { result, options } = stateRef.current;
    if (!result) return;
    const mode = result.mode === 'interview' ? 'interview' : 'practice';
    const { material } = result.drill;
    if (material.kind === 'code') start({ material: 'code', language: material.language, mode });
    else if (options.programId) start({ material: 'passage', mode });
    else reset();
  }, [reset, start]);

  const setOptions = useCallback((patch: Partial<DrillOptions>) => dispatch({ type: 'options', patch }), [dispatch]);
  const add = useCallback((input: FindingInput) => dispatch({ type: 'add', input }), [dispatch]);
  const update = useCallback((id: string, input: FindingInput) => dispatch({ type: 'update', id, input }), [dispatch]);
  const remove = useCallback((id: string) => dispatch({ type: 'remove', id }), [dispatch]);
  const move = useCallback((id: string, delta: -1 | 1) => dispatch({ type: 'move', id, delta }), [dispatch]);
  const editFix = useCallback((path: string, text: string) => dispatch({ type: 'edit-fix', path, text }), [dispatch]);
  const discardFix = useCallback(() => dispatch({ type: 'discard-fix' }), [dispatch]);
  const handIn = useCallback(() => void submit(), [submit]);
  const gradeAgain = useCallback(() => void regrade(), [regrade]);

  return {
    stage,
    options: state.options,
    setOptions,
    start,
    /** Gives up waiting for a build; it goes on without the screen. */
    stopWaiting: reset,
    /** Back to the choices from a screen that could not open its attempt. */
    restart: reset,
    building: state.building,
    failure: state.failure,
    retry,
    session,
    result: state.result,
    regrading: state.regrading,
    gradingMessage: state.gradingMessage,
    regrade: gradeAgain,
    nextDrill,
    review: {
      findings,
      fixEdits,
      changedPaths: session ? changedFiles(session.drill.files, fixEdits).map((file) => file.path) : [],
      submitting: stage === 'submitting',
      add,
      update,
      remove,
      move,
      editFix,
      discardFix,
      submit: handIn,
    },
  };
}

export type CodeReviewDrill = ReturnType<typeof useCodeReviewDrill>;
