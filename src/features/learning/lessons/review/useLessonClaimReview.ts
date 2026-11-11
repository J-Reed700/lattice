import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningClaimReviewKey, learningLessonEvidencePrefix, learningLessonQuestionKey, learningPlanKey, learningProgramKey } from '@/features/learning/api/learningQueryKeys';
import { learningSourcesKey } from '@/features/learning/sources/useLearningSources';
import type {
  ClaimActionRequestDto,
  LearningGenerationJob,
  LessonClaimReviewDto,
  LessonQuestionDto,
  LessonQuestionEditDto,
  LessonSectionEditDto,
  RemoveLearningTeachingSentencesResultDto,
  ResolveLearningClaimResultDto,
  SnapshotNoteDto,
  TeachingSettleDto,
} from '@/lib/bindings';
import { failureToError } from '@/lib/errorHandling';
import type { ApiResult } from '@/types';

function unwrap<T>(result: ApiResult<T>): T {
  if (!result.ok) throw failureToError(result);
  return result.data;
}

const operationId = () => crypto.randomUUID();

/** The lesson's claim review; null before its first verification pass. */
export function useLessonClaimReview(programId: string, lessonId: string, enabled = true) {
  const api = useApi();
  return useQuery<LessonClaimReviewDto | null>({
    queryKey: learningClaimReviewKey(programId, lessonId),
    queryFn: async () => unwrap(await api.getLearningLessonClaimReview(programId, lessonId)),
    enabled: enabled && Boolean(programId && lessonId),
    retry: false,
  });
}

async function refreshAfterAction(client: QueryClient, programId: string, lessonId: string) {
  await Promise.all([
    client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
    client.invalidateQueries({ queryKey: learningLessonEvidencePrefix(programId) }),
    client.invalidateQueries({ queryKey: learningProgramKey(programId) }),
  ]);
}

export type ResolveClaimVariables = { claimId: string; action: ClaimActionRequestDto; expectedRevision: number };

/**
 * Every claim action goes through `resolve_learning_claim` with the review's
 * revision; the review reloads after success and after a refusal, so a stale
 * claim never stays on screen. Adding a source has its own command (`useAddSource`).
 */
export function useResolveLearningClaim(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation<ResolveLearningClaimResultDto, Error, ResolveClaimVariables>({
    retry: false,
    mutationFn: async ({ claimId, action, expectedRevision }) => unwrap(await api.resolveLearningClaim({
      programId, lessonId, claimId, action, expectedRevision, operationId: operationId(),
    })),
    onSettled: () => refreshAfterAction(client, programId, lessonId),
  });
}

export type ResolveManyVariables = { claimIds: string[]; action: ClaimActionRequestDto; expectedRevision: number };

/**
 * One action on many claims through `resolve_learning_claims`: all of them or
 * none, with one revision. The review reloads after success and after a
 * refusal.
 */
export function useResolveLearningClaims(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation<ResolveLearningClaimResultDto, Error, ResolveManyVariables>({
    retry: false,
    mutationFn: async ({ claimIds, action, expectedRevision }) => unwrap(await api.resolveLearningClaims({
      programId, lessonId, claimIds, action, expectedRevision, operationId: operationId(),
    })),
    onSettled: () => refreshAfterAction(client, programId, lessonId),
  });
}

/**
 * Takes the sentences of teaching notes out of a published lesson through
 * `remove_learning_teaching_sentences`: all of them or none, with one
 * revision. The review reloads after success and after a refusal.
 */
export function useRemoveTeachingSentences(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation<RemoveLearningTeachingSentencesResultDto, Error, { findingKeys: string[]; expectedRevision: number }>({
    retry: false,
    mutationFn: async ({ findingKeys, expectedRevision }) => unwrap(await api.removeLearningTeachingSentences({
      programId, lessonId, findingKeys, expectedRevision,
    })),
    onSettled: () => refreshAfterAction(client, programId, lessonId),
  });
}

/**
 * "Keep as written" on teaching findings, or taking that back: all the
 * findings change or none; the review reloads either way.
 */
export function useSettleTeachingFindings(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation<null, Error, { findingKeys: string[]; settle: TeachingSettleDto }>({
    retry: false,
    mutationFn: async ({ findingKeys, settle }) => unwrap(await api.settleLearningTeachingFindings({ programId, lessonId, findingKeys, settle })),
    onSettled: () => client.invalidateQueries({ queryKey: learningClaimReviewKey(programId, lessonId) }),
  });
}

export type EditSectionVariables = {
  sectionIndex: number;
  beforeTitle: string;
  beforeBody: string;
  title: string;
  body: string;
};

/**
 * Saves the user's rewrite of one section of a published lesson. The course,
 * its evidence and the review reload after success and after a refusal, so a
 * section that changed underneath never stays on screen as it was.
 */
export function useEditLearningLessonSection(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation<LessonSectionEditDto, Error, EditSectionVariables>({
    retry: false,
    mutationFn: async (variables) => unwrap(await api.editLearningLessonSection({ programId, lessonId, ...variables, dryRun: false })),
    onSettled: () => refreshAfterAction(client, programId, lessonId),
  });
}

/** A published lesson's question as the editor opens it; read fresh each time the editor opens. */
export function useLearningLessonQuestion(programId: string, lessonId: string, questionIndex: number) {
  const api = useApi();
  return useQuery<LessonQuestionDto>({
    queryKey: learningLessonQuestionKey(programId, lessonId, questionIndex),
    queryFn: async () => unwrap(await api.getLearningLessonQuestion(programId, lessonId, questionIndex)),
    staleTime: Infinity,
    gcTime: 0,
    retry: false,
  });
}

export type EditQuestionVariables = {
  questionIndex: number;
  before: LessonQuestionDto;
  after: LessonQuestionDto;
};

/**
 * Saves the user's rewrite of one question of a published lesson. The course,
 * its evidence and the review reload after success and after a refusal, as
 * for a section.
 */
export function useEditLearningLessonQuestion(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation<LessonQuestionEditDto, Error, EditQuestionVariables>({
    retry: false,
    mutationFn: async (variables) => unwrap(await api.editLearningLessonQuestion({ programId, lessonId, ...variables, dryRun: false })),
    onSettled: () => refreshAfterAction(client, programId, lessonId),
  });
}

/**
 * Resume verification (or Retry a stopped review). `courseRevision` is the
 * course's revision, which the job action checks.
 */
export function useResumeLearningLessonReview(programId: string, lessonId: string) {
  const api = useApi();
  const client = useQueryClient();
  return useMutation<LearningGenerationJob, Error, { courseRevision: number }>({
    retry: false,
    mutationFn: async ({ courseRevision }) => unwrap(await api.resumeLearningLessonReview({
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
  const api = useApi();
  const client = useQueryClient();
  return useMutation<SnapshotNoteDto, Error, { sourceId: string; versionId: string }>({
    retry: false,
    mutationFn: async ({ sourceId, versionId }) =>
      unwrap(await api.captureLearningSourceSnapshot({ programId, sourceId, versionId })),
    onSuccess: () => client.invalidateQueries({ queryKey: learningSourcesKey(programId) }),
  });
}
