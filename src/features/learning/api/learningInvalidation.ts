import { LEARNING_PROGRAMS_KEY, learningAssessmentWorkspaceKey, learningCanvasKey, learningLessonEvidencePrefix, learningMemoryKey, learningOutlineEvidencePrefix, learningPlanKey, learningPracticalWorkspaceKey, learningPracticeWorkspaceKey, learningProgramKey, learningRecallKey } from '@/features/learning/api/learningQueryKeys';

import type { QueryClient } from '@tanstack/react-query';

/** Source revisions are independent of curriculum revisions. Refresh every
 * source-derived view, including inactive queries reopened during staleTime. */
export async function invalidateLearningSourceDependents(client: QueryClient, programId: string) {
  await Promise.all([
    LEARNING_PROGRAMS_KEY,
    learningProgramKey(programId),
    learningPlanKey(programId),
    learningLessonEvidencePrefix(programId),
    learningOutlineEvidencePrefix(programId),
    ['learning-source-version', programId],
    ['learning-source-search', programId],
    ['learning-semantic-sources', programId],
  ].map((queryKey) => client.invalidateQueries({ queryKey })));
}

/** Imports replace complete aggregates, not just source material. Clear detail
 * snapshots too: the same session/form ID can now contain different data or no
 * longer exist. Cancel old reads before refreshing so they cannot win the race. */
export async function refreshLearningProgramAfterImport(client: QueryClient, programId: string) {
  const workspaces = [
    learningMemoryKey(programId),
    learningCanvasKey(programId),
    learningRecallKey(programId),
    learningPracticeWorkspaceKey(programId),
    learningPracticalWorkspaceKey(programId),
    learningAssessmentWorkspaceKey(programId),
  ];
  const details = {
    predicate: (query: { queryKey: readonly unknown[]; state: { data: unknown } }) => {
      if (!['learning-practice-session', 'learning-assessment-form'].includes(String(query.queryKey[0]))) return false;
      // A pending first read has no owner yet; cancel it conservatively as well.
      const data = query.state.data as { programId?: string } | undefined;
      return !data || data.programId === programId;
    },
  };
  await Promise.all([
    ...workspaces.map((queryKey) => client.cancelQueries({ queryKey })),
    client.cancelQueries(details),
  ]);
  await Promise.all([
    invalidateLearningSourceDependents(client, programId),
    client.invalidateQueries({ queryKey: ['learning-sources', programId] }),
    ...workspaces.map((queryKey) => client.resetQueries({ queryKey })),
    client.resetQueries(details),
    // A replaced program can also replace its linked study deck.
    client.invalidateQueries({ queryKey: ['study'] }),
  ]);
}
