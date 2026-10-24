import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';

import {
  LEARNING_CHECKER_ACCURACY_KEY,
  LEARNING_SOURCE_TRUST_CALIBRATION_KEY,
  LEARNING_SOURCE_TRUST_OVERRIDES_KEY,
  LEARNING_CLAIM_REVIEW_ROOT,
  LEARNING_SOURCE_TRUST_PREFIX,
  learningSourceTrustKey,
} from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type {
  CheckerAccuracyDto,
  ExportedProvenanceEvalExamplesDto,
  GetLearningSourceTrustRequestDto,
  LearningClaimProfileDto,
  LearningSiteTrustOverrideDto,
  LearningSourceTrustCalibrationDto,
  LearningSourceTrustViewDto,
  LearningTrustChangeResultDto,
  LearningTrustOverrideInputDto,
  LearningTrustOverrideScopeDto,
  ReassessLearningSourceTrustRequestDto,
  ReprofileLearningClaimRequestDto,
} from '@/lib/bindings';
import type { ApiResult } from '@/types';

function unwrap<T>(result: ApiResult<T>): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

/**
 * A trust change moves levels in every course a site appears in, and sends
 * waiting claims back to be re-decided: refresh every trust read, the
 * override list and every claim review.
 */
async function afterTrustChange(client: QueryClient) {
  await Promise.all([
    client.invalidateQueries({ queryKey: LEARNING_SOURCE_TRUST_PREFIX }),
    client.invalidateQueries({ queryKey: LEARNING_SOURCE_TRUST_OVERRIDES_KEY }),
    client.invalidateQueries({ queryKey: LEARNING_CLAIM_REVIEW_ROOT }),
  ]);
}

/** Levels and reasons of a course's sources, or of one claim's supports; never calls a model. */
export function useSourceTrust(request: GetLearningSourceTrustRequestDto | null) {
  return useQuery<LearningSourceTrustViewDto>({
    queryKey: request ? learningSourceTrustKey(request) : [...LEARNING_SOURCE_TRUST_PREFIX, 'idle'],
    queryFn: async () => unwrap(await VaultAPI.getLearningSourceTrust(request!)),
    enabled: Boolean(request?.programId),
    staleTime: 5_000,
    retry: false,
  });
}

export function useSetSourceTrustOverride() {
  const client = useQueryClient();
  return useMutation<LearningTrustChangeResultDto, Error, LearningTrustOverrideInputDto>({
    retry: false,
    mutationFn: async (request) => unwrap(await VaultAPI.setSourceTrustOverride(request)),
    onSuccess: () => afterTrustChange(client),
  });
}

export function useClearSourceTrustOverride() {
  const client = useQueryClient();
  return useMutation<LearningTrustChangeResultDto | null, Error, LearningTrustOverrideScopeDto>({
    retry: false,
    mutationFn: async (scope) => unwrap(await VaultAPI.clearSourceTrustOverride(scope)),
    onSuccess: () => afterTrustChange(client),
  });
}

/** Judges a source's publisher again with the Learning model. */
export function useReassessSourceTrust() {
  const client = useQueryClient();
  return useMutation<LearningSourceTrustViewDto, Error, ReassessLearningSourceTrustRequestDto>({
    retry: false,
    mutationFn: async (request) => unwrap(await VaultAPI.reassessLearningSourceTrust(request)),
    onSuccess: () => client.invalidateQueries({ queryKey: LEARNING_SOURCE_TRUST_PREFIX }),
  });
}

/** Profiles a claim again with the Learning model. */
export function useReprofileClaim() {
  const client = useQueryClient();
  return useMutation<LearningClaimProfileDto, Error, ReprofileLearningClaimRequestDto>({
    retry: false,
    mutationFn: async (request) => unwrap(await VaultAPI.reprofileLearningClaim(request)),
    onSuccess: () => client.invalidateQueries({ queryKey: LEARNING_SOURCE_TRUST_PREFIX }),
  });
}

/** Site and site-for-subject overrides, with what the claims under them did. */
export function useSourceTrustOverrides(enabled = true) {
  return useQuery<LearningSiteTrustOverrideDto[]>({
    queryKey: LEARNING_SOURCE_TRUST_OVERRIDES_KEY,
    queryFn: async () => unwrap(await VaultAPI.listSourceTrustOverrides()),
    enabled,
    retry: false,
  });
}

/** The bars in force and why (§13). */
export function useSourceTrustCalibration(enabled = true) {
  return useQuery<LearningSourceTrustCalibrationDto>({
    queryKey: LEARNING_SOURCE_TRUST_CALIBRATION_KEY,
    queryFn: async () => unwrap(await VaultAPI.getSourceTrustCalibration()),
    enabled,
    retry: false,
  });
}

/** The local checker's measured reliability on the user's labelled claims (CR §15). */
export function useCheckerReliability(enabled = true) {
  return useQuery<CheckerAccuracyDto[]>({
    queryKey: LEARNING_CHECKER_ACCURACY_KEY,
    queryFn: async () => unwrap(await VaultAPI.getLearningCheckerAccuracy()),
    enabled,
    retry: false,
  });
}

/** `null` when the user dismissed the save dialog. */
export function useExportProvenanceEval() {
  return useMutation<ExportedProvenanceEvalExamplesDto | null, Error, void>({
    retry: false,
    mutationFn: async () => unwrap(await VaultAPI.exportProvenanceEvalExamples()),
  });
}
