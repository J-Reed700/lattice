import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { learningClaimReviewKey, learningLessonAuditKey, learningPlanKey, learningProgramKey, learningVerifierAuditKey } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type { LearningLessonAuditDto, LearningLessonAuditResultDto, LearningVerifierAuditDto } from '@/lib/bindings';
import type { ApiResult } from '@/types';

function unwrap<T>(result: ApiResult<T>): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

/** The course's opt-in to the Verifier audit, with what consent it needs (§14.2). */
export function useProgramVerifierAudit(programId: string, enabled = true) {
  return useQuery<LearningVerifierAuditDto>({
    queryKey: learningVerifierAuditKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningProgramVerifierAudit(programId)),
    enabled: enabled && Boolean(programId),
    retry: false,
  });
}

export function useSetProgramVerifierAudit(programId: string) {
  const client = useQueryClient();
  return useMutation<LearningVerifierAuditDto, Error, { on: boolean; webSearch: boolean; consentConfirmed: boolean }>({
    retry: false,
    mutationFn: async ({ on, webSearch, consentConfirmed }) =>
      unwrap(await VaultAPI.setLearningProgramVerifierAudit(programId, on, webSearch, consentConfirmed)),
    onSuccess: (audit) => client.setQueryData(learningVerifierAuditKey(programId), audit),
  });
}

/** The lesson's latest audit: batches, tokens, the last error. Null before any. */
export function useLessonAudit(programId: string, lessonId: string, enabled = true) {
  return useQuery<LearningLessonAuditDto | null>({
    queryKey: learningLessonAuditKey(programId, lessonId),
    queryFn: async () => unwrap(await VaultAPI.getLearningLessonAudit(programId, lessonId)),
    enabled: enabled && Boolean(programId && lessonId),
    retry: false,
  });
}

/** "Audit this lesson now" on a ready lesson; a dispute sends it back to Needs review. */
export function useAuditLessonNow(programId: string, lessonId: string) {
  const client = useQueryClient();
  return useMutation<LearningLessonAuditResultDto, Error, void>({
    retry: false,
    mutationFn: async () => unwrap(await VaultAPI.auditLearningLessonNow(programId, lessonId, crypto.randomUUID())),
    onSettled: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: learningLessonAuditKey(programId, lessonId) }),
        client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
        client.invalidateQueries({ queryKey: learningVerifierAuditKey(programId) }),
        client.invalidateQueries({ queryKey: learningPlanKey(programId) }),
        client.invalidateQueries({ queryKey: learningProgramKey(programId) }),
      ]);
    },
  });
}
