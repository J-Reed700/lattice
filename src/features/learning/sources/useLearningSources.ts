import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import type { WaitingClaimsCheck } from '@/features/learning/api/client';
import { invalidateLearningSourceDependents } from '@/features/learning/api/learningInvalidation';
import type {
  AddLearningDocumentSourceRequestDto,
  AddLearningTextSourceRequestDto,
  AddLearningWebSourceRequestDto,
  AdoptLearningSourceVersionRequestDto,
  GetLearningSourceVersionRequestDto,
  LearningSourceSearchResultDto,
  LearningSourceVersionDto,
  LearningSourceWorkspaceDto,
  RefreshLearningSourceRequestDto,
  SearchLearningSourcesRequestDto,
  UpdateLearningSourcePolicyRequestDto,
} from '@/lib/bindings';


export const learningSourcesKey = (programId: string) => ['learning-sources', programId] as const;
export const learningSourceVersionKey = (request: GetLearningSourceVersionRequestDto | null) =>
  ['learning-source-version', request?.programId, request?.sourceId, request?.versionId] as const;
export const learningSourceSearchKey = (request: SearchLearningSourcesRequestDto | null) =>
  ['learning-source-search', request?.programId, request?.query, request?.limit] as const;

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningSourceWorkspace(programId: string) {
  const api = useApi();
  return useQuery<LearningSourceWorkspaceDto>({
    queryKey: learningSourcesKey(programId),
    queryFn: async () => unwrap(await api.getLearningSourceWorkspace(programId)),
    enabled: Boolean(programId),
    staleTime: 5_000,
    retry: false,
  });
}

export function useLearningSourceVersion(request: GetLearningSourceVersionRequestDto | null) {
  const api = useApi();
  return useQuery<LearningSourceVersionDto>({
    queryKey: learningSourceVersionKey(request),
    queryFn: async () => unwrap(await api.getLearningSourceVersion(request!)),
    enabled: Boolean(request?.programId && request.sourceId && request.versionId),
    staleTime: 60_000,
    retry: false,
  });
}

export function useSearchLearningSources(request: SearchLearningSourcesRequestDto | null) {
  const api = useApi();
  return useQuery<LearningSourceSearchResultDto[]>({
    queryKey: learningSourceSearchKey(request),
    queryFn: async () => unwrap(await api.searchLearningSources(request!)),
    enabled: Boolean(request?.programId && request.query.trim()),
    staleTime: 5_000,
    retry: false,
  });
}

function useSourceMutation<TRequest>(
  mutation: (request: TRequest) => Promise<{ ok: true; data: LearningSourceWorkspaceDto } | { ok: false; error: string }>,
) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutation(request)),
    onSuccess: async (workspace) => {
      client.setQueryData(learningSourcesKey(workspace.programId), workspace);
      await invalidateLearningSourceDependents(client, workspace.programId);
    },
  });
}

export const useAddLearningWebSource = (recheck?: WaitingClaimsCheck) => {
  const api = useApi();
  return useSourceMutation<AddLearningWebSourceRequestDto>((request) => api.addLearningWebSource(request, recheck));
};
export const useAddLearningDocumentSource = (recheck?: WaitingClaimsCheck) => {
  const api = useApi();
  return useSourceMutation<AddLearningDocumentSourceRequestDto>((request) => api.addLearningDocumentSource(request, recheck));
};
export const useAddLearningTextSource = (recheck?: WaitingClaimsCheck) => {
  const api = useApi();
  return useSourceMutation<AddLearningTextSourceRequestDto>((request) => api.addLearningTextSource(request, recheck));
};
export const useRefreshLearningSource = () => {
  const api = useApi();
  return useSourceMutation<RefreshLearningSourceRequestDto>(api.refreshLearningSource);
};
export const useAdoptLearningSourceVersion = () => {
  const api = useApi();
  return useSourceMutation<AdoptLearningSourceVersionRequestDto>(api.adoptLearningSourceVersion);
};
export const useUpdateLearningSourcePolicy = () => {
  const api = useApi();
  return useSourceMutation<UpdateLearningSourcePolicyRequestDto>(api.updateLearningSourcePolicy);
};
