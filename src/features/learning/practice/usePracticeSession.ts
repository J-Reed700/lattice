import { newId } from '@/lib/api';
import { useCallback, useEffect, useRef, useState } from 'react';

import { isConflictError } from '@/features/learning/practice/shared/useAutosave';
import {
  useAcceptLearningPracticeProposal,
  useChangeLearningPracticeMode,
  useLearningPracticeSession,
  useLearningPracticeWorkspace,
  useOpenLearningPracticeSource,
  useRejectLearningPracticeProposal,
  useRequestLearningTutorResponse,
  useRevealLearningPracticeSolution,
  useSaveLearningPracticeArtifact,
  useStartLearningPracticeSession,
  useSubmitLearningPracticeAttempt,
} from '@/features/learning/practice/useLearningPractice';
import type {
  LearningLessonDto,
  LearningPracticeHintLevel,
  LearningPracticeMode,
  LearningPracticeTaskKind,
  LearningProgramDto,
  LearningSourceLibraryItemDto,
  SaveLearningPracticeArtifactRequestDto,
} from '@/lib/bindings';

export type PracticeBusyState =
  | 'idle'
  | 'starting'
  | 'saving'
  | 'changing_mode'
  | 'tutor'
  | 'opening_source'
  | 'revealing'
  | 'submitting'
  | 'deciding';

export interface UsePracticeSessionOptions {
  program: LearningProgramDto;
  lesson: LearningLessonDto | undefined;
  taskKind?: LearningPracticeTaskKind;
  embedded?: boolean;
}


export function isConflict(error: unknown): boolean {
  if (isConflictError(error)) return true;
  const message = error instanceof Error ? error.message : String(error);
  return /changed|conflict|revision|reload/i.test(message);
}

export function usePracticeSession({
  program,
  lesson,
  taskKind = 'independent',
  embedded = false,
}: UsePracticeSessionOptions) {
  const programId = program.summary.id;
  const lessonId = lesson?.id ?? '';

  const workspace = useLearningPracticeWorkspace(programId);
  const start = useStartLearningPracticeSession();
  const save = useSaveLearningPracticeArtifact();
  const changeMode = useChangeLearningPracticeMode();
  const openSource = useOpenLearningPracticeSource();
  const tutor = useRequestLearningTutorResponse();
  const reveal = useRevealLearningPracticeSolution();
  const submit = useSubmitLearningPracticeAttempt();
  const acceptProposal = useAcceptLearningPracticeProposal();
  const rejectProposal = useRejectLearningPracticeProposal();

  const [sessionId, setSessionId] = useState<string | null>(null);
  const sessionQuery = useLearningPracticeSession(sessionId);
  const session = sessionQuery.data;

  const [modeChoice, setModeChoice] = useState<LearningPracticeMode>(embedded ? 'practice' : 'explore');
  const [busyState, setBusyState] = useState<PracticeBusyState>('idle');
  const [actionError, setActionError] = useState('');
  const [conflict, setConflict] = useState(false);
  const [retryAction, setRetryAction] = useState<{ label: string; run: () => Promise<void> } | null>(null);

  const [solutionConfirm, setSolutionConfirm] = useState(false);
  const [selectedSource, setSelectedSource] = useState<{ sourceId: string; versionId: string } | null>(null);
  const [hintUsed, setHintUsed] = useState<LearningPracticeHintLevel[]>([]);

  const conflictRef = useRef(false);
  const revisionRef = useRef(0);
  const saveRequestRef = useRef<{ fingerprint: string; request: SaveLearningPracticeArtifactRequestDto } | null>(null);

  const summary = session?.summary;
  const active = summary?.status === 'active';
  const mode = summary?.mode ?? modeChoice;
  const canTutor = active && mode !== 'demonstrate';
  const solutionAllowed = active && mode === 'explore';
  const sourceOpeningAllowed = !active || mode !== 'demonstrate';

  // Synchronize active session with workspace
  useEffect(() => {
    const candidates =
      workspace.data?.sessions.filter(
        (item) => item.lessonId === lessonId && (item.taskKind ?? 'independent') === taskKind,
      ) ?? [];
    const latest = [...candidates].sort(
      (a, b) => Number(b.status === 'active') - Number(a.status === 'active') || b.updatedAt - a.updatedAt,
    )[0];
    if (
      sessionId &&
      sessionQuery.data?.summary.lessonId === lessonId &&
      (sessionQuery.data?.summary.taskKind ?? 'independent') === taskKind
    ) {
      return;
    }
    if (latest?.id !== sessionId) setSessionId(latest?.id ?? null);
  }, [lessonId, sessionId, sessionQuery.data?.summary.lessonId, sessionQuery.data?.summary.taskKind, workspace.data, taskKind]);

  // Synchronize revision and used hints
  useEffect(() => {
    if (!session) return;
    if (session.summary.id !== sessionId) return;
    revisionRef.current = session.summary.revision;

    setHintUsed(
      session.assistance
        .filter((event) => event.kind === 'hint')
        .map((event) => {
          const val = (event.details as Record<string, unknown> | null)?.hintLevel;
          return typeof val === 'string' ? (val as LearningPracticeHintLevel) : undefined;
        })
        .filter((value): value is LearningPracticeHintLevel => Boolean(value)),
    );
  }, [session, sessionId]);

  const perform = useCallback(async (state: PracticeBusyState, run: () => Promise<unknown>) => {
    setBusyState(state);
    setActionError('');
    setRetryAction(null);
    setConflict(false);
    try {
      await run();
      conflictRef.current = false;
      return true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const isConf = isConflict(error);
      conflictRef.current = isConf;
      setActionError(message);
      setConflict(isConf);
      return false;
    } finally {
      setBusyState('idle');
    }
  }, []);

  const runRetryable = useCallback(
    async <T,>(
      state: PracticeBusyState,
      label: string,
      invoke: (request: T) => Promise<unknown>,
      request: T,
    ) => {
      const stableRun = async () => invoke(request);
      const execute = async () => {
        const ok = await perform(state, stableRun);
        if (ok) {
          try {
            if (sessionId) await sessionQuery.refetch();
            await workspace.refetch();
          } finally {
            setBusyState('idle');
          }
        } else if (!conflictRef.current) {
          const retry = {
            label: `Retry ${label.toLowerCase()}`,
            run: async () => {
              await execute();
            },
          };
          setRetryAction(() => retry);
        }
        return ok;
      };
      return await execute();
    },
    [perform, sessionId, sessionQuery, workspace],
  );

  const startSession = useCallback(async () => {
    if (lesson?.preparation !== 'ready') return;
    const id = newId();
    const request = {
      operationId: newId(),
      sessionId: id,
      programId,
      lessonId: lesson.id,
      expectedProgramRevision: program.summary.revision,
      mode: session?.summary.status === 'submitted' ? ('practice' as const) : session?.summary.mode ?? modeChoice,
      taskKind,
      revisesSessionId: session?.summary.status === 'submitted' ? session.summary.id : null,
    };
    await runRetryable('starting', 'Start attempt', async (payload) => {
      await start.mutateAsync(payload);
      setSessionId(id);
    }, request);
  }, [lesson, program.summary.revision, programId, session, modeChoice, taskKind, runRetryable, start]);

  const switchMode = useCallback(
    async (next: LearningPracticeMode, preSave?: () => Promise<boolean>) => {
      if (!summary || !active || next === mode) return;
      if (preSave && !(await preSave())) return;
      const latest = sessionQuery.data?.summary ?? summary;
      const request = {
        operationId: newId(),
        programId,
        sessionId: latest.id,
        expectedRevision: revisionRef.current || latest.revision,
        mode: next,
      };
      if (
        await runRetryable('changing_mode', 'Change mode', (payload) => changeMode.mutateAsync(payload), request)
      ) {
        setModeChoice(next);
      }
    },
    [summary, active, mode, sessionQuery.data?.summary, programId, runRetryable, changeMode],
  );

  const sendTutor = useCallback(
    async (
      requestKind: 'hint' | 'question' | 'critique',
      prompt: string,
      hintLevel?: LearningPracticeHintLevel,
      preSave?: () => Promise<boolean>,
    ) => {
      if (!summary || !active || !canTutor || !prompt.trim()) return false;
      if (preSave && !(await preSave())) return false;
      const latest = sessionQuery.data?.summary ?? summary;
      const request = {
        operationId: newId(),
        programId,
        sessionId: latest.id,
        expectedRevision: revisionRef.current || latest.revision,
        requestKind,
        prompt: prompt.trim(),
        hintLevel: hintLevel ?? null,
      };
      const label = requestKind === 'hint' ? 'Request hint' : requestKind === 'critique' ? 'Request critique' : 'Ask tutor';
      const ok = await runRetryable('tutor', label, (payload) => tutor.mutateAsync(payload), request);
      if (ok && hintLevel) {
        setHintUsed((current) => (current.includes(hintLevel) ? current : [...current, hintLevel]));
      }
      return ok;
    },
    [summary, active, canTutor, sessionQuery.data?.summary, programId, runRetryable, tutor],
  );

  const doOpenSource = useCallback(
    async (source: LearningSourceLibraryItemDto, versionId: string, preSave?: () => Promise<boolean>) => {
      if (!summary || busyState !== 'idle') return;
      if (!active) {
        setSelectedSource({ sourceId: source.id, versionId });
        return;
      }
      if (mode === 'demonstrate') return;
      if (preSave && !(await preSave())) return;
      const latest = sessionQuery.data?.summary ?? summary;
      const request = {
        operationId: newId(),
        programId,
        sessionId: latest.id,
        expectedRevision: revisionRef.current || latest.revision,
        sourceId: source.id,
        versionId,
      };
      if (
        await runRetryable('opening_source', 'Open saved source', (payload) => openSource.mutateAsync(payload), request)
      ) {
        setSelectedSource({ sourceId: source.id, versionId });
      }
    },
    [summary, busyState, active, mode, sessionQuery.data?.summary, programId, runRetryable, openSource],
  );

  const doReveal = useCallback(
    async (preSave?: () => Promise<boolean>) => {
      if (!summary || !solutionAllowed) return;
      if (preSave && !(await preSave())) return;
      const latest = sessionQuery.data?.summary ?? summary;
      const request = {
        operationId: newId(),
        programId,
        sessionId: latest.id,
        expectedRevision: revisionRef.current || latest.revision,
      };
      if (
        await runRetryable('revealing', 'Reveal solution', (payload) => reveal.mutateAsync(payload), request)
      ) {
        setSolutionConfirm(false);
      }
    },
    [summary, solutionAllowed, sessionQuery.data?.summary, programId, runRetryable, reveal],
  );

  const doSubmit = useCallback(
    async (text: string, preSave?: () => Promise<boolean>) => {
      if (!summary || !active || !text.trim()) return;
      if (preSave && !(await preSave())) return;
      const latest = sessionQuery.data?.summary ?? summary;
      const request = {
        operationId: newId(),
        programId,
        sessionId: latest.id,
        expectedRevision: revisionRef.current || latest.revision,
      };
      await runRetryable('submitting', 'Submit response', (payload) => submit.mutateAsync(payload), request);
    },
    [summary, active, sessionQuery.data?.summary, programId, runRetryable, submit],
  );

  const decide = useCallback(
    async (proposalId: string, accepted: boolean, preSave?: () => Promise<boolean>) => {
      if (!summary || !active) return;
      if (preSave && !(await preSave())) return;
      const latest = sessionQuery.data?.summary ?? summary;
      const request = {
        operationId: newId(),
        programId,
        sessionId: latest.id,
        expectedRevision: revisionRef.current || latest.revision,
        proposalId,
      };
      await runRetryable(
        'deciding',
        accepted ? 'Accept proposal' : 'Reject proposal',
        (payload) => (accepted ? acceptProposal.mutateAsync(payload) : rejectProposal.mutateAsync(payload)),
        request,
      );
    },
    [summary, active, sessionQuery.data?.summary, programId, runRetryable, acceptProposal, rejectProposal],
  );

  const busyLabel =
    busyState === 'starting'
      ? 'Starting attempt…'
      : busyState === 'saving'
        ? 'Saving response'
        : busyState === 'changing_mode'
          ? 'Changing mode…'
          : busyState === 'tutor'
            ? 'The tutor is responding…'
            : busyState === 'opening_source'
              ? 'Opening source…'
              : busyState === 'revealing'
                ? 'Revealing solution…'
                : busyState === 'submitting'
                  ? 'Submitting response'
                  : busyState === 'deciding'
                    ? 'Updating idea…'
                    : '';

  return {
    sessionId,
    setSessionId,
    sessionQuery,
    session,
    summary,
    active,
    mode,
    modeChoice,
    setModeChoice,
    canTutor,
    solutionAllowed,
    sourceOpeningAllowed,
    busyState,
    busyLabel,
    actionError,
    setActionError,
    conflict,
    setConflict,
    retryAction,
    setRetryAction,
    solutionConfirm,
    setSolutionConfirm,
    selectedSource,
    setSelectedSource,
    hintUsed,
    setHintUsed,
    revisionRef,
    saveRequestRef,
    perform,
    startSession,
    switchMode,
    sendTutor,
    doOpenSource,
    doReveal,
    doSubmit,
    decide,
    workspace,
    saveMutation: save,
  };
}
