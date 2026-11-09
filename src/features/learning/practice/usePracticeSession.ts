import { useEffect, useRef, useState } from 'react';

import { useRetry } from '@/features/learning/practice/shared/useRetry';
import { useAutosave } from '@/features/learning/practice/shared/useAutosave';
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
import { newId } from '@/lib/api';
import { registerPendingSave } from '@/lib/pendingSaves';

export type PracticeBusyState =
  | 'idle'
  | 'starting'
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
  attemptId?: string | null;
}


export function usePracticeSession({
  program,
  lesson,
  taskKind = 'independent',
  embedded = false,
  attemptId = null,
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
  const [seenAttemptId, setSeenAttemptId] = useState<string | null>(null);
  if (attemptId !== seenAttemptId) {
    setSeenAttemptId(attemptId);
    if (attemptId) setSessionId(attemptId);
  }
  const sessionQuery = useLearningPracticeSession(sessionId);
  const session = sessionQuery.data;

  const [modeChoice, setModeChoice] = useState<LearningPracticeMode>(embedded ? 'practice' : 'explore');
  const [busyState, setBusyState] = useState<PracticeBusyState>('idle');
  const [failedKind, setFailedKind] = useState<PracticeBusyState | null>(null);
  const [text, setTextState] = useState('');
  const [hydratedFor, setHydratedFor] = useState<string | null>(null);

  const [solutionConfirm, setSolutionConfirm] = useState(false);
  const [selectedSource, setSelectedSource] = useState<{ sourceId: string; versionId: string } | null>(null);
  const [hintUsed, setHintUsed] = useState<LearningPracticeHintLevel[]>([]);

  const revisionRef = useRef(0);
  const textRef = useRef('');
  const savedTextRef = useRef('');
  const saveRequestRef = useRef<{ fingerprint: string; request: SaveLearningPracticeArtifactRequestDto } | null>(null);

  const summary = session?.summary;
  const active = summary?.status === 'active';
  const mode = summary?.mode ?? modeChoice;
  const canTutor = active && mode !== 'demonstrate';
  const solutionAllowed = active && mode === 'explore';
  const sourceOpeningAllowed = !active || mode !== 'demonstrate';

  // Synchronize active session with workspace
  useEffect(() => {
    if (attemptId && sessionId === attemptId) return;
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
  }, [attemptId, lessonId, sessionId, sessionQuery.data?.summary.lessonId, sessionQuery.data?.summary.taskKind, workspace.data, taskKind]);

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

  useEffect(() => {
    if (!session || session.summary.id !== sessionId || hydratedFor === sessionId) return;
    textRef.current = session.artifact.text;
    savedTextRef.current = session.artifact.text;
    saveRequestRef.current = null;
    setTextState(session.artifact.text);
    setHydratedFor(session.summary.id);
  }, [session, sessionId, hydratedFor]);

  const setText = (value: string) => {
    textRef.current = value;
    setTextState(value);
  };

  const saveArtifact = async (value: string, expectedRevision: number) => {
    if (!summary) throw new Error('There is no attempt to save.');
    const fingerprint = `${summary.id}:${expectedRevision}:${value}`;
    if (saveRequestRef.current?.fingerprint !== fingerprint) {
      saveRequestRef.current = {
        fingerprint,
        request: { operationId: newId(), programId, sessionId: summary.id, expectedRevision, text: value },
      };
    }
    const request = saveRequestRef.current.request;
    const updated = await save.mutateAsync(request);
    const updatedSummary = updated.sessions.find((item) => item.id === request.sessionId);
    const revision = updatedSummary?.revision ?? expectedRevision;
    revisionRef.current = revision;
    savedTextRef.current = value;
    saveRequestRef.current = null;
    await sessionQuery.refetch();
    return { revision };
  };

  const autosave = useAutosave({
    value: text,
    revision: summary?.revision ?? 0,
    save: saveArtifact,
    enabled: Boolean(active) && hydratedFor === sessionId,
  });

  const saveNow = async (): Promise<boolean> => {
    if (!summary || !active || textRef.current === savedTextRef.current) return true;
    await autosave.flush();
    return textRef.current === savedTextRef.current;
  };
  const saveNowRef = useRef(saveNow);
  saveNowRef.current = saveNow;
  useEffect(() => registerPendingSave(() => saveNowRef.current()), []);

  const action = async (kind: PracticeBusyState, invoke: () => Promise<unknown>) => {
    setBusyState(kind);
    try {
      await invoke();
      if (sessionId) await sessionQuery.refetch();
      await workspace.refetch();
      setFailedKind(null);
      return true;
    } catch (error) {
      setFailedKind(kind);
      throw error;
    } finally {
      setBusyState('idle');
    }
  };
  const actions = useRetry(action);
  const runAction = async (kind: PracticeBusyState, invoke: () => Promise<unknown>) => (await actions.run(kind, invoke)) === true;

  const reloadAttempt = async () => {
    await Promise.all([sessionQuery.refetch(), workspace.refetch()]);
    actions.clear();
    setFailedKind(null);
    await autosave.flush();
  };

  const startSession = async () => {
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
    await runAction('starting', async () => {
      await start.mutateAsync(request);
      setSessionId(id);
    });
  };

  const switchMode = async (next: LearningPracticeMode, preSave?: () => Promise<boolean>) => {
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
    await runAction('changing_mode', async () => {
      await changeMode.mutateAsync(request);
      setModeChoice(next);
    });
  };

  const sendTutor = async (
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
    return runAction('tutor', async () => {
      await tutor.mutateAsync(request);
      if (hintLevel) setHintUsed((current) => (current.includes(hintLevel) ? current : [...current, hintLevel]));
    });
  };

  const doOpenSource = async (source: LearningSourceLibraryItemDto, versionId: string, preSave?: () => Promise<boolean>) => {
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
    await runAction('opening_source', async () => {
      await openSource.mutateAsync(request);
      setSelectedSource({ sourceId: source.id, versionId });
    });
  };

  const doReveal = async (preSave?: () => Promise<boolean>) => {
    if (!summary || !solutionAllowed) return;
    if (preSave && !(await preSave())) return;
    const latest = sessionQuery.data?.summary ?? summary;
    const request = {
      operationId: newId(),
      programId,
      sessionId: latest.id,
      expectedRevision: revisionRef.current || latest.revision,
    };
    await runAction('revealing', async () => {
      await reveal.mutateAsync(request);
      setSolutionConfirm(false);
    });
  };

  const doSubmit = async (value: string, preSave?: () => Promise<boolean>) => {
    if (!summary || !active || !value.trim()) return;
    if (preSave && !(await preSave())) return;
    const latest = sessionQuery.data?.summary ?? summary;
    const request = {
      operationId: newId(),
      programId,
      sessionId: latest.id,
      expectedRevision: revisionRef.current || latest.revision,
    };
    await runAction('submitting', () => submit.mutateAsync(request));
  };

  const decide = async (proposalId: string, accepted: boolean, preSave?: () => Promise<boolean>) => {
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
    await runAction('deciding', () => (accepted ? acceptProposal.mutateAsync(request) : rejectProposal.mutateAsync(request)));
  };

  const busyLabel =
    busyState === 'starting'
      ? 'Starting attempt…'
      : busyState === 'changing_mode'
          ? 'Changing help…'
          : busyState === 'tutor'
            ? 'The tutor is responding…'
            : busyState === 'opening_source'
              ? 'Opening source…'
              : busyState === 'revealing'
                ? 'Revealing solution…'
                : busyState === 'submitting'
                  ? 'Submitting response…'
                  : busyState === 'deciding'
                    ? 'Updating idea…'
                    : '';

  return {
    text,
    setText,
    dirty: text !== (session?.artifact.text ?? ''),
    autosave,
    saveNow,
    reloadAttempt,
    busy: busyState !== 'idle',
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
    failure: actions.error === null ? null : { kind: failedKind, message: actions.error, retry: actions.conflict ? undefined : actions.retry },
    conflict: actions.conflict || autosave.state === 'conflict',
    solutionConfirm,
    setSolutionConfirm,
    selectedSource,
    setSelectedSource,
    hintUsed,
    startSession,
    switchMode,
    sendTutor,
    doOpenSource,
    doReveal,
    doSubmit,
    decide,
    workspace,
  };
}
