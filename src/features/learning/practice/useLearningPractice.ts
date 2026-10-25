import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningPracticeWorkspaceKey } from '@/features/learning/api/learningQueryKeys';
import type VaultAPI from '@/lib/api';
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
  const api = useApi();
  return useQuery<LearningPracticeWorkspaceDto>({
    queryKey: learningPracticeWorkspaceKey(programId),
    queryFn: async () => unwrap(await api.getLearningPracticeWorkspace(programId)),
    enabled: Boolean(programId),
    staleTime: 3_000,
    retry: false,
  });
}

export function useLearningPracticeSession(sessionId: string | null) {
  const api = useApi();
  return useQuery<LearningPracticeSessionDto>({
    queryKey: learningPracticeSessionKey(sessionId),
    queryFn: async () => unwrap(await api.getLearningPracticeSession(sessionId!)),
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

export const useStartLearningPracticeSession = () => {
  const api = useApi();
  return usePracticeMutation<StartLearningPracticeSessionRequestDto>(api.startLearningPracticeSession);
};
export const useSaveLearningPracticeArtifact = () => {
  const api = useApi();
  return usePracticeMutation<SaveLearningPracticeArtifactRequestDto>(api.saveLearningPracticeArtifact);
};
export const useChangeLearningPracticeMode = () => {
  const api = useApi();
  return usePracticeMutation<ChangeLearningPracticeModeRequestDto>(api.changeLearningPracticeMode);
};
export const useOpenLearningPracticeSource = () => {
  const api = useApi();
  return usePracticeMutation<OpenLearningPracticeSourceRequestDto>(api.openLearningPracticeSource);
};
export const useRequestLearningTutorResponse = () => {
  const api = useApi();
  return usePracticeMutation<RequestLearningTutorResponseRequestDto>(api.requestLearningTutorResponse);
};
export const useRevealLearningPracticeSolution = () => {
  const api = useApi();
  return usePracticeMutation<RevealLearningPracticeSolutionRequestDto>(api.revealLearningPracticeSolution);
};
export const useSubmitLearningPracticeAttempt = () => {
  const api = useApi();
  return usePracticeMutation<SubmitLearningPracticeAttemptRequestDto>(api.submitLearningPracticeAttempt);
};
export const useAcceptLearningPracticeProposal = () => {
  const api = useApi();
  return usePracticeMutation<DecideLearningPracticeProposalRequestDto>(api.acceptLearningPracticeProposal);
};
export const useRejectLearningPracticeProposal = () => {
  const api = useApi();
  return usePracticeMutation<DecideLearningPracticeProposalRequestDto>(api.rejectLearningPracticeProposal);
};
