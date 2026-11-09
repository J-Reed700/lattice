import { useCallback, useEffect, useRef, useState } from 'react';

import { isJobFinished, useJobStatus, type JobDto } from '@/features/jobs/api';
import type { StartedCodeReviewDto } from '@/lib/bindings';

import { drillsApi } from './api';
import { DRILL_BUILD, DRILL_GRADING, shelfOf } from './drillJobs';
import {
  materialOf,
  readChoices,
  readCourseLanguage,
  sizeOf,
  validMinutes,
  writeChoices,
  writeCourseLanguage,
  type DrillOptions,
} from './drillOptions';
import { initialState, limitOf, reduce, type DrillAction, type DrillState } from './drillState';
import { changedFiles, toFindingDto, type FindingInput } from './findings';
import { clearReviewDraft, EMPTY_DRAFT, readReviewDraft, writeReviewDraft } from './reviewDraft';

/** How often a build that is waited for is asked about again, in case the report that ended it was missed. */
const BUILD_POLL_MS = 30_000;
/** How often an attempt that waits for its grade is read again, in case the report that ended its job was missed. */
const GRADING_POLL_MS = 10_000;

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
 * write, the hand-in and the wait for its grade, and the reveal. An attempt the learner left is read again from the
 * backend when they open it from the recent drills.
 */
export function useCodeReviewDrill(programId: string) {
  const [state, setState] = useState<DrillState>(() =>
    initialState('choose', { ...readChoices(), language: readCourseLanguage(programId) }),
  );
  // The reducer runs when an action is dispatched, so the state asynchronous work reads is never behind a render.
  const stateRef = useRef(state);
  const attemptRef = useRef<string | null>(null);
  // Each thing the learner does that supersedes the last takes a new run; results of an older run are dropped.
  const runRef = useRef(0);
  const mounted = useRef(false);
  const asking = useRef(false);
  const askAgain = useRef(false);

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

  /** The attempt the screen holds. */
  const holdAttempt = useCallback((id: string | null) => {
    attemptRef.current = id;
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
          issues: sizeOf(stateRef.current.options.size, stateRef.current.options.material).issues,
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
        dispatch({ type: 'failed', failure: { message: 'Choose the language of the drill.', retry: 'start' } });
        return;
      }
      const { lines, issues } = sizeOf(options.size, options.material);
      try {
        const started = await drillsApi.startCourseDrill(programId, options.lessonId, material, options.mode, lines, issues);
        if (!live(run)) return;
        if (started.status === 'started') open(started);
        else dispatch({ type: 'building', jobId: started.jobId });
      } catch (error) {
        if (live(run)) dispatch({ type: 'failed', failure: { message: messageOf(error), retry: 'start' } });
      }
    },
    [dispatch, live, open, programId],
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
      const material = materialOf(options);
      if (!material) return;
      runRef.current += 1;
      writeChoices(options);
      if (material.kind === 'code') writeCourseLanguage(programId, material.language);
      attemptRef.current = null;
      dispatch({ type: 'starting' });
      void ask();
    },
    [ask, dispatch, programId],
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

  useJobStatus((job) => {
    const current = stateRef.current;
    if (current.stage === 'starting' && !current.failure && job.kind === DRILL_BUILD) {
      const ours = current.building?.jobId === job.id;
      if (!ours && job.subjectId !== shelfOf(programId, current.options)) return;
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

  /** Another drill on the same material and in the mode of the one just done, on whichever lesson is next. */
  const nextDrill = useCallback(() => {
    const { result } = stateRef.current;
    if (!result) return;
    const mode = result.mode === 'interview' ? 'interview' : 'practice';
    const { material } = result.drill;
    if (material.kind === 'code') start({ material: 'code', language: material.language, mode, lessonId: null });
    else start({ material: 'passage', mode, lessonId: null });
  }, [start]);

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
    /** Opens an attempt from the recent list. */
    restore: (id: string) => {
      void restore(id);
    },
    /** Back to the choices; a build under way goes on without the screen. */
    toChoices: reset,
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
