import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { learningPracticeWorkspaceKey } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type {
  ChangeLearningPracticeModeRequestDto,
  DecideLearningPracticeProposalRequestDto,
  LearningPracticeSessionDto,
  LearningPracticeWorkspaceDto,
  OpenLearningPracticeSourceRequestDto,
  RequestLearningTutorResponseRequestDto,
  RevealLearningPracticeSolutionRequestDto,
  SaveLearningPracticeArtifactRequestDto,
  StartLearningPracticeSessionRequestDto,
  SubmitLearningPracticeAttemptRequestDto,
} from '@/lib/bindings';

export { learningPracticeWorkspaceKey } from '@/features/learning/api/learningQueryKeys';
export const learningPracticeSessionKey = (sessionId: string | null) => ['learning-practice-session', sessionId] as const;

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningPracticeWorkspace(programId: string) {
  return useQuery<LearningPracticeWorkspaceDto>({
    queryKey: learningPracticeWorkspaceKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningPracticeWorkspace(programId)),
    enabled: Boolean(programId),
    staleTime: 3_000,
    retry: false,
  });
}

export function useLearningPracticeSession(sessionId: string | null) {
  return useQuery<LearningPracticeSessionDto>({
    queryKey: learningPracticeSessionKey(sessionId),
    queryFn: async () => unwrap(await VaultAPI.getLearningPracticeSession(sessionId!)),
    enabled: Boolean(sessionId),
    staleTime: 0,
    retry: false,
  });
}

function usePracticeMutation<TRequest>(mutate: (request: TRequest) => ReturnType<typeof VaultAPI.getLearningPracticeWorkspace>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutate(request)),
    onSuccess: async (workspace) => {
      client.setQueryData(learningPracticeWorkspaceKey(workspace.programId), workspace);
      // Session mutation responses are workspace summaries; the session detail
      // query remains the authority for artifact, tutor, and result content.
      await client.invalidateQueries({ queryKey: ['learning-practice-session'] });
    },
  });
}

export const useStartLearningPracticeSession = () => usePracticeMutation<StartLearningPracticeSessionRequestDto>(VaultAPI.startLearningPracticeSession);
export const useSaveLearningPracticeArtifact = () => usePracticeMutation<SaveLearningPracticeArtifactRequestDto>(VaultAPI.saveLearningPracticeArtifact);
export const useChangeLearningPracticeMode = () => usePracticeMutation<ChangeLearningPracticeModeRequestDto>(VaultAPI.changeLearningPracticeMode);
export const useOpenLearningPracticeSource = () => usePracticeMutation<OpenLearningPracticeSourceRequestDto>(VaultAPI.openLearningPracticeSource);
export const useRequestLearningTutorResponse = () => usePracticeMutation<RequestLearningTutorResponseRequestDto>(VaultAPI.requestLearningTutorResponse);
export const useRevealLearningPracticeSolution = () => usePracticeMutation<RevealLearningPracticeSolutionRequestDto>(VaultAPI.revealLearningPracticeSolution);
export const useSubmitLearningPracticeAttempt = () => usePracticeMutation<SubmitLearningPracticeAttemptRequestDto>(VaultAPI.submitLearningPracticeAttempt);
export const useAcceptLearningPracticeProposal = () => usePracticeMutation<DecideLearningPracticeProposalRequestDto>(VaultAPI.acceptLearningPracticeProposal);
export const useRejectLearningPracticeProposal = () => usePracticeMutation<DecideLearningPracticeProposalRequestDto>(VaultAPI.rejectLearningPracticeProposal);
