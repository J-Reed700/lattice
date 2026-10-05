import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { LEARNING_PROGRAMS_KEY, learningPlanKey, learningProgramKey } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type {
  LearningCurriculumRevisionActionRequestDto,
  DiscardLearningCurriculumRevisionRequestDto,
  LearningPlanDto,
  LearningGenerationJobActionRequestDto,
  LearningGenerationJob,
  LearningDiagnosticAttemptDto,
  PreviewLearningCurriculumRevisionRequestDto,
  SkipLearningDiagnosticRequestDto,
  StartLearningDiagnosticRequestDto,
  SubmitLearningDiagnosticRequestDto,
} from '@/lib/bindings';


export { learningPlanKey } from '@/features/learning/api/learningQueryKeys';
function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
export function useLearningPlan(programId: string, preparing = false) {
  return useQuery<LearningPlanDto>({
    queryKey: learningPlanKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningPlan(programId)),
    enabled: Boolean(programId),
    staleTime: 3_000,
    refetchInterval: (query) => preparing || query.state.data?.jobs?.some((job) => ['pending', 'running'].includes(job.status)) ? 2_000 : false,
    refetchIntervalInBackground: true,
    refetchOnMount: 'always',
    retry: false,
  });
}
function usePlanMutation<TRequest>(mutation: (request: TRequest) => ReturnType<typeof VaultAPI.getLearningPlan>, changesProgram = false) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutation(request)),
    onSuccess: async (plan) => {
      client.setQueryData(learningPlanKey(plan.programId), plan);
      if (changesProgram) {
        await Promise.all([
          client.invalidateQueries({ queryKey: learningProgramKey(plan.programId) }),
          client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY }),
        ]);
      }
    },
  });
}
function usePlanRefreshMutation<TRequest, TResult>(mutation: (request: TRequest) => Promise<{ ok: true; data: TResult } | { ok: false; error: string }>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutation(request)),
    onSuccess: async (_result, request) => {
      const programId = (request as { programId: string }).programId;
      await client.invalidateQueries({ queryKey: learningPlanKey(programId) });
    },
  });
}
export const usePreviewLearningCurriculumRevision = () => usePlanMutation<PreviewLearningCurriculumRevisionRequestDto>(VaultAPI.previewLearningCurriculumRevision);
export const useAcceptLearningCurriculumRevision = () => usePlanMutation<LearningCurriculumRevisionActionRequestDto>(VaultAPI.acceptLearningCurriculumRevision, true);
export const useDiscardLearningCurriculumRevision = () => usePlanMutation<DiscardLearningCurriculumRevisionRequestDto>(VaultAPI.discardLearningCurriculumRevision);
export const useStartLearningDiagnostic = () => usePlanRefreshMutation<StartLearningDiagnosticRequestDto, LearningDiagnosticAttemptDto>(VaultAPI.startLearningDiagnostic);
export const useSubmitLearningDiagnostic = () => usePlanRefreshMutation<SubmitLearningDiagnosticRequestDto, LearningDiagnosticAttemptDto>(VaultAPI.submitLearningDiagnostic);
export const useSkipLearningDiagnostic = () => usePlanRefreshMutation<SkipLearningDiagnosticRequestDto, LearningDiagnosticAttemptDto>(VaultAPI.skipLearningDiagnostic);
export const useCancelLearningGenerationJob = () => usePlanRefreshMutation<LearningGenerationJobActionRequestDto, LearningGenerationJob>(VaultAPI.cancelLearningGenerationJob);
export const useRetryLearningGenerationJob = () => usePlanRefreshMutation<LearningGenerationJobActionRequestDto, LearningGenerationJob>(VaultAPI.retryLearningGenerationJob);
