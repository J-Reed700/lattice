import { useRef, useState } from 'react';

import { useMutation, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { invalidateLearningSourceDependents } from '@/features/learning/api/learningInvalidation';
import { learningClaimReviewKey, learningLessonEvidencePrefix } from '@/features/learning/api/learningQueryKeys';
import { learningSourcesKey } from '@/features/learning/sources/useLearningSources';
import type { AttachEvidenceDto, AttachScopeDto, ClaimAttachProgressDto, ClaimAttachReportDto } from '@/lib/bindings';

export type AddSourceVariables = {
  expectedRevision: number;
  evidence: AttachEvidenceDto;
} & (
  | {
    /** The claim the attach is recorded on: the first of the scope. */
    claimId: string;
    scope: AttachScopeDto;
  }
  /** The lesson as a whole, with no claim chosen first. */
  | { lesson: true }
);

/**
 * "Add a source" over `attach_learning_claim_evidence`, or over
 * `attach_learning_lesson_source` for the whole lesson (§3.6, §5.3): the
 * stage it is at, live, and the report when it ends. No timeout: the fetch's
 * own failure or the checker's error ends it. Cancel stops listening; the
 * server finishes the claim it is on (the ledger is append-only).
 */
export function useAddSource(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  const [progress, setProgress] = useState<ClaimAttachProgressDto | null>(null);
  // Each run gets a token; a cancelled run's late events and result are dropped.
  const run = useRef(0);

  const mutation = useMutation<ClaimAttachReportDto | null, Error, AddSourceVariables>({
    retry: false,
    mutationFn: async (variables) => {
      const token = ++run.current;
      setProgress(null);
      const { expectedRevision, evidence } = variables;
      const onProgress = (update: ClaimAttachProgressDto) => { if (run.current === token) setProgress(update); };
      const result = 'lesson' in variables
        ? await api.attachLearningLessonSource({ programId, lessonId, expectedRevision, evidence }, onProgress)
        : await api.attachLearningClaimEvidence({ programId, lessonId, claimId: variables.claimId, expectedRevision, evidence, scope: variables.scope }, onProgress);
      if (run.current !== token) return null;
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    onSettled: async () => {
      // An attach saves a source: every source-derived view changes with it.
      await Promise.all([
        client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
        client.invalidateQueries({ queryKey: learningLessonEvidencePrefix(programId) }),
        client.invalidateQueries({ queryKey: learningSourcesKey(programId) }),
        invalidateLearningSourceDependents(client, programId),
      ]);
    },
  });

  const cancel = () => {
    run.current += 1;
    setProgress(null);
    mutation.reset();
  };

  return { ...mutation, progress, cancel };
}
