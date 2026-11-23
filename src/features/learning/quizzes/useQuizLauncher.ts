import { useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import {
  EMPTY_PROGRAM_QUIZ_LAUNCHER,
  scopeKey,
  useQuizLauncherStore,
} from '@/features/learning/quizzes/quizLauncherStore';
import { usePrepareQuizBank, useStartQuiz } from '@/features/learning/quizzes/useQuizzes';
import type { LearningQuizMode, LearningQuizStatusDto } from '@/lib/bindings';

export { scopeKey };

/**
 * Starting, resuming, retaking and preparing a quiz, shared by the overview
 * rows and the callouts. State persists in `useQuizLauncherStore` per program so
 * tab switches preserve busy flags, errors, and Prepare-all loops.
 */
export function useQuizLauncher(programId: string, onOpen: (formId: string) => void) {
  const api = useApi();
  const client = useQueryClient();
  const start = useStartQuiz();
  const prepare = usePrepareQuizBank();
  const state = useQuizLauncherStore((store) => store.programs[programId] ?? EMPTY_PROGRAM_QUIZ_LAUNCHER);
  const takeOperationId = useQuizLauncherStore((store) => store.takeOperationId);
  const clearOperationId = useQuizLauncherStore((store) => store.clearOperationId);
  const runScope = useQuizLauncherStore((store) => store.runScope);
  const runPrepareAll = useQuizLauncherStore((store) => store.prepareAll);

  const begin = (status: LearningQuizStatusDto, mode: LearningQuizMode, retakeOfFormId: string | null = null) =>
    runScope(programId, status, async () => {
      const key = `start:${scopeKey(status.scope)}:${mode}:${retakeOfFormId ?? 'new'}`;
      const form = await start.mutateAsync({
        operationId: takeOperationId(programId, key),
        formId: takeOperationId(programId, `${key}:form`),
        programId,
        scope: status.scope,
        mode,
        retakeOfFormId,
      });
      clearOperationId(programId, key);
      clearOperationId(programId, `${key}:form`);
      onOpen(form.id);
    });

  /** A new attempt beside the open one; the open one stays as it is, ungraded. */
  const startFresh = (status: LearningQuizStatusDto) =>
    runScope(programId, status, async () => {
      const key = `start:${scopeKey(status.scope)}:graded:fresh`;
      const form = await start.mutateAsync({
        operationId: takeOperationId(programId, key),
        formId: takeOperationId(programId, `${key}:form`),
        programId,
        scope: status.scope,
        mode: 'graded',
        retakeOfFormId: status.latest?.formId ?? null,
      });
      clearOperationId(programId, key);
      clearOperationId(programId, `${key}:form`);
      onOpen(form.id);
    });

  const prepareBank = (status: LearningQuizStatusDto, topUp = false) =>
    runScope(programId, status, async () => {
      const key = `prepare:${scopeKey(status.scope)}:${topUp}`;
      await prepare.mutateAsync({
        operationId: takeOperationId(programId, key),
        programId,
        scope: status.scope,
        topUp,
      });
      clearOperationId(programId, key);
    });

  const prepareAll = (statuses: LearningQuizStatusDto[]) =>
    runPrepareAll(programId, statuses, api, client);

  const retakeOf = (status: LearningQuizStatusDto) => status.latest?.formId ?? status.best?.formId ?? null;

  return {
    begin,
    startFresh,
    prepareBank,
    prepareAll,
    preparingAll: state.preparingAll,
    retake: (status: LearningQuizStatusDto, mode: LearningQuizMode = 'graded') => begin(status, mode, retakeOf(status)),
    resume: (status: LearningQuizStatusDto) => { if (status.activeFormId) onOpen(status.activeFormId); },
    isBusy: (status: LearningQuizStatusDto) => Boolean(state.busyScopes[scopeKey(status.scope)]),
    errorFor: (status: LearningQuizStatusDto) => state.errorsByScope[scopeKey(status.scope)],
  };
}

export type QuizLauncher = ReturnType<typeof useQuizLauncher>;
