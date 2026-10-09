import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { invalidateLearningSourceDependents } from '@/features/learning/api/learningInvalidation';
import VaultAPI from '@/lib/api';
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
  return useQuery<LearningSourceWorkspaceDto>({
    queryKey: learningSourcesKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningSourceWorkspace(programId)),
    enabled: Boolean(programId),
    staleTime: 5_000,
    retry: false,
  });
}

export function useLearningSourceVersion(request: GetLearningSourceVersionRequestDto | null) {
  return useQuery<LearningSourceVersionDto>({
    queryKey: learningSourceVersionKey(request),
    queryFn: async () => unwrap(await VaultAPI.getLearningSourceVersion(request!)),
    enabled: Boolean(request?.programId && request.sourceId && request.versionId),
    staleTime: 60_000,
    retry: false,
  });
}

export function useSearchLearningSources(request: SearchLearningSourcesRequestDto | null) {
  return useQuery<LearningSourceSearchResultDto[]>({
    queryKey: learningSourceSearchKey(request),
    queryFn: async () => unwrap(await VaultAPI.searchLearningSources(request!)),
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

export const useAddLearningWebSource = () => useSourceMutation<AddLearningWebSourceRequestDto>(VaultAPI.addLearningWebSource);
export const useAddLearningDocumentSource = () => useSourceMutation<AddLearningDocumentSourceRequestDto>(VaultAPI.addLearningDocumentSource);
export const useAddLearningTextSource = () => useSourceMutation<AddLearningTextSourceRequestDto>(VaultAPI.addLearningTextSource);
export const useRefreshLearningSource = () => useSourceMutation<RefreshLearningSourceRequestDto>(VaultAPI.refreshLearningSource);
export const useAdoptLearningSourceVersion = () => useSourceMutation<AdoptLearningSourceVersionRequestDto>(VaultAPI.adoptLearningSourceVersion);
export const useUpdateLearningSourcePolicy = () => useSourceMutation<UpdateLearningSourcePolicyRequestDto>(VaultAPI.updateLearningSourcePolicy);
