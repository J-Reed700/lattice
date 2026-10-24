import { useRef, useState } from 'react';

import { useMutation, useQueryClient } from '@tanstack/react-query';

import { invalidateLearningSourceDependents } from '@/features/learning/api/learningInvalidation';
import { learningClaimReviewKey, learningLessonEvidencePrefix } from '@/features/learning/api/learningQueryKeys';
import { learningSourcesKey } from '@/features/learning/sources/useLearningSources';
import VaultAPI from '@/lib/api';
import type { AttachEvidenceDto, AttachScopeDto, ClaimAttachProgressDto, ClaimAttachReportDto } from '@/lib/bindings';

export type AddSourceVariables = {
  /** The claim the attach is recorded on: the first of the scope. */
  claimId: string;
  expectedRevision: number;
  evidence: AttachEvidenceDto;
  scope: AttachScopeDto;
};

/**
 * "Add a source" over `attach_learning_claim_evidence` (§3.6, §5.3): the
 * stage it is at, live, and the report when it ends. No timeout: the fetch's
 * own failure or the checker's error ends it. Cancel stops listening; the
 * server finishes the claim it is on (the ledger is append-only).
 */
export function useAddSource(programId: string, lessonId: string) {
  const client = useQueryClient();
  const [progress, setProgress] = useState<ClaimAttachProgressDto | null>(null);
  // Each run gets a token; a cancelled run's late events and result are dropped.
  const run = useRef(0);

  const mutation = useMutation<ClaimAttachReportDto | null, Error, AddSourceVariables>({
    retry: false,
    mutationFn: async ({ claimId, expectedRevision, evidence, scope }) => {
      const token = ++run.current;
      setProgress(null);
      const result = await VaultAPI.attachLearningClaimEvidence(
        { programId, lessonId, claimId, expectedRevision, evidence, scope },
        (update) => { if (run.current === token) setProgress(update); },
      );
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
