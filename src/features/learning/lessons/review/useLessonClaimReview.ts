import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';

import { learningClaimReviewKey, learningLessonEvidencePrefix, learningPlanKey } from '@/features/learning/api/learningQueryKeys';
import { learningSourcesKey } from '@/features/learning/sources/useLearningSources';
import VaultAPI from '@/lib/api';
import type {
  ClaimActionRequestDto,
  LearningGenerationJob,
  LessonClaimReviewDto,
  ResolveLearningClaimResultDto,
  SnapshotNoteDto,
} from '@/lib/bindings';
import type { ApiResult } from '@/types';

function unwrap<T>(result: ApiResult<T>): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

const operationId = () => crypto.randomUUID();

/** The lesson's claim review; null before its first verification pass. */
export function useLessonClaimReview(programId: string, lessonId: string, enabled = true) {
  return useQuery<LessonClaimReviewDto | null>({
    queryKey: learningClaimReviewKey(programId, lessonId),
    queryFn: async () => unwrap(await VaultAPI.getLearningLessonClaimReview(programId, lessonId)),
    enabled: enabled && Boolean(programId && lessonId),
    retry: false,
  });
}

async function refreshAfterAction(client: QueryClient, programId: string, lessonId: string) {
  await Promise.all([
    client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
    client.invalidateQueries({ queryKey: learningLessonEvidencePrefix(programId) }),
  ]);
}

export type ResolveClaimVariables = { claimId: string; action: ClaimActionRequestDto; expectedRevision: number };

/**
 * Every claim action goes through `resolve_learning_claim` with the review's
 * revision; the review reloads after success and after a refusal, so a stale
 * claim never stays on screen. Adding a source has its own command (`useAddSource`).
 */
export function useResolveLearningClaim(programId: string, lessonId: string) {
  const client = useQueryClient();
  return useMutation<ResolveLearningClaimResultDto, Error, ResolveClaimVariables>({
    retry: false,
    mutationFn: async ({ claimId, action, expectedRevision }) => unwrap(await VaultAPI.resolveLearningClaim({
      programId, lessonId, claimId, action, expectedRevision, operationId: operationId(),
    })),
    onSettled: () => refreshAfterAction(client, programId, lessonId),
  });
}

/**
 * "Keep as written" on a teaching finding: it stops waiting for the user;
 * the review reloads either way.
 */
export function useKeepTeachingFinding(programId: string, lessonId: string) {
  const client = useQueryClient();
  return useMutation<void, Error, { findingKey: string }>({
    retry: false,
    mutationFn: async ({ findingKey }) => unwrap(await VaultAPI.keepLearningTeachingFinding({ programId, lessonId, findingKey })),
    onSettled: () => client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
  });
}

/**
 * Resume verification (or Retry a stopped review). `courseRevision` is the
 * course's revision, which the job action checks.
 */
export function useResumeLearningLessonReview(programId: string, lessonId: string) {
  const client = useQueryClient();
  return useMutation<LearningGenerationJob, Error, { courseRevision: number }>({
    retry: false,
    mutationFn: async ({ courseRevision }) => unwrap(await VaultAPI.resumeLearningLessonReview({
      programId, lessonId, expectedRevision: courseRevision, operationId: operationId(),
    })),
    onSettled: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: learningPlanKey(programId) }),
        client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
      ]);
    },
  });
}

/**
 * "Capture again" for a frozen copy that failed (§5.4), named by the attach
 * report's source and version.
 */
export function useCaptureSourceSnapshot(programId: string) {
  const client = useQueryClient();
  return useMutation<SnapshotNoteDto, Error, { sourceId: string; versionId: string }>({
    retry: false,
    mutationFn: async ({ sourceId, versionId }) =>
      unwrap(await VaultAPI.captureLearningSourceSnapshot({ programId, sourceId, versionId })),
    onSuccess: () => client.invalidateQueries({ queryKey: learningSourcesKey(programId) }),
  });
}
