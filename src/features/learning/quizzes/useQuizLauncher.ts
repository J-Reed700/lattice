import { useState } from 'react';

import { useOperationIds, usePrepareQuizBank, useStartQuiz } from '@/features/learning/quizzes/useQuizzes';
import type { LearningQuizMode, LearningQuizScopeDto, LearningQuizStatusDto } from '@/lib/bindings';

export const scopeKey = (scope: LearningQuizScopeDto) => `${scope.kind}:${scope.id ?? 'course'}`;

const message = (error: unknown) => (error instanceof Error ? error.message : 'That did not work. Try again.');

/**
 * Starting, resuming, retaking and preparing a quiz, shared by the overview
 * rows and the callouts. Each request keeps its operation and form ids until
 * it lands, so a retry after a lost response cannot start a second form.
 */
export function useQuizLauncher(programId: string, onOpen: (formId: string) => void) {
  const start = useStartQuiz();
  const prepare = usePrepareQuizBank();
  const operations = useOperationIds();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<{ scope: string; message: string } | null>(null);

  const run = async (status: LearningQuizStatusDto, work: () => Promise<void>) => {
    const key = scopeKey(status.scope);
    setBusy(key);
    setError(null);
    try { await work(); } catch (cause) { setError({ scope: key, message: message(cause) }); } finally { setBusy(null); }
  };

  const begin = (status: LearningQuizStatusDto, mode: LearningQuizMode, retakeOfFormId: string | null = null) => run(status, async () => {
    const key = `start:${scopeKey(status.scope)}:${mode}:${retakeOfFormId ?? 'new'}`;
    const form = await start.mutateAsync({ operationId: operations.take(key), formId: operations.take(`${key}:form`), programId, scope: status.scope, mode, retakeOfFormId });
    operations.done(key);
    operations.done(`${key}:form`);
    onOpen(form.id);
  });

  /** A new attempt beside the open one; the open one stays as it is, ungraded. */
  const startFresh = (status: LearningQuizStatusDto) => run(status, async () => {
    const key = `start:${scopeKey(status.scope)}:graded:fresh`;
    const form = await start.mutateAsync({ operationId: operations.take(key), formId: operations.take(`${key}:form`), programId, scope: status.scope, mode: 'graded', retakeOfFormId: status.latest?.formId ?? null });
    operations.done(key);
    operations.done(`${key}:form`);
    onOpen(form.id);
  });

  const prepareBank = (status: LearningQuizStatusDto, topUp = false) => run(status, async () => {
    const key = `prepare:${scopeKey(status.scope)}:${topUp}`;
    await prepare.mutateAsync({ operationId: operations.take(key), programId, scope: status.scope, topUp });
    operations.done(key);
  });

  const retakeOf = (status: LearningQuizStatusDto) => status.latest?.formId ?? status.best?.formId ?? null;

  return {
    begin,
    startFresh,
    prepareBank,
    retake: (status: LearningQuizStatusDto, mode: LearningQuizMode = 'graded') => begin(status, mode, retakeOf(status)),
    resume: (status: LearningQuizStatusDto) => { if (status.activeFormId) onOpen(status.activeFormId); },
    isBusy: (status: LearningQuizStatusDto) => busy === scopeKey(status.scope),
    errorFor: (status: LearningQuizStatusDto) => (error?.scope === scopeKey(status.scope) ? error.message : undefined),
  };
}

export type QuizLauncher = ReturnType<typeof useQuizLauncher>;
