import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type {
  ApplyLearningPackImportRequestDto,
  CancelLearningPackImportPreviewRequestDto,
  ChangeLearningRecallSchedulerRequestDto,
  CreateLearningSourceSelectorRequestDto,
  DecideLearningRecallDuplicateRequestDto,
  DeleteLearningSourceRequestDto,
  ExportLearningPackRequestDto,
  LearningPortabilityWorkspaceDto,
  LearningRecallWorkspaceDto,
  LearningSourceSelectorDto,
  LearningSourceSemanticSearchResultDto,
  LearningSourceWorkspaceDto,
  PreviewLearningPackImportRequestDto,
  ReimportLearningSourceRequestDto,
  ReviewLearningRecallCardRequestDto,
  SaveLearningRecallCardRequestDto,
  SearchLearningSourcesSemanticallyRequestDto,
} from '@/lib/bindings';

export const learningPortabilityKey = (programId: string) => ['learning-portability', programId] as const;
export const learningRecallKey = (programId: string) => ['learning-recall-v2', programId] as const;
export const semanticSourcesKey = (request: SearchLearningSourcesSemanticallyRequestDto | null) =>
  ['learning-semantic-sources', request?.programId, request?.query, request?.limit] as const;

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningPortability(programId: string) {
  return useQuery<LearningPortabilityWorkspaceDto>({
    queryKey: learningPortabilityKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningPortabilityWorkspace(programId)),
    enabled: Boolean(programId), retry: false, staleTime: 3_000,
  });
}

export function useLearningRecallV2(programId: string) {
  return useQuery<LearningRecallWorkspaceDto>({
    queryKey: learningRecallKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningRecallWorkspace(programId)),
    enabled: Boolean(programId), retry: false, staleTime: 2_000,
  });
}

function usePortabilityMutation<TRequest, TResult>(
  mutation: (request: TRequest) => Promise<{ ok: true; data: TResult } | { ok: false; error: string }>,
  programId: string,
) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutation(request)),
    onSuccess: async (data) => {
      if (data && typeof data === 'object' && 'programId' in data && data.programId === programId) {
        if ('exports' in data) client.setQueryData(learningPortabilityKey(programId), data);
        if ('sources' in data) client.invalidateQueries({ queryKey: ['learning-sources', programId] });
        if ('cards' in data) client.setQueryData(learningRecallKey(programId), data);
      }
      await client.invalidateQueries({ queryKey: learningPortabilityKey(programId) });
    },
  });
}

export const useExportLearningPack = (programId: string) => usePortabilityMutation<ExportLearningPackRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.exportLearningPack, programId);
export const usePreviewLearningPackImport = (programId: string) => usePortabilityMutation<PreviewLearningPackImportRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.previewLearningPackImport, programId);
export const useApplyLearningPackImport = (programId: string) => usePortabilityMutation<ApplyLearningPackImportRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.applyLearningPackImport, programId);
export const useCancelLearningPackImportPreview = (programId: string) => usePortabilityMutation<CancelLearningPackImportPreviewRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.cancelLearningPackImportPreview, programId);
export const useDeleteLearningSourceV2 = (programId: string) => usePortabilityMutation<DeleteLearningSourceRequestDto, LearningSourceWorkspaceDto>(VaultAPI.deleteLearningSource, programId);
export const useReimportLearningSource = (programId: string) => usePortabilityMutation<ReimportLearningSourceRequestDto, LearningSourceWorkspaceDto>(VaultAPI.reimportLearningSource, programId);
export const useCreateLearningSourceSelector = (programId: string) => usePortabilityMutation<CreateLearningSourceSelectorRequestDto, LearningSourceSelectorDto>(VaultAPI.createLearningSourceSelector, programId);
export const useSaveLearningRecallCard = (programId: string) => usePortabilityMutation<SaveLearningRecallCardRequestDto, LearningRecallWorkspaceDto>(VaultAPI.saveLearningRecallCard, programId);
export const useDecideLearningRecallDuplicate = (programId: string) => usePortabilityMutation<DecideLearningRecallDuplicateRequestDto, LearningRecallWorkspaceDto>(VaultAPI.decideLearningRecallDuplicate, programId);
export const useChangeLearningRecallScheduler = (programId: string) => usePortabilityMutation<ChangeLearningRecallSchedulerRequestDto, LearningRecallWorkspaceDto>(VaultAPI.changeLearningRecallScheduler, programId);
export const useReviewLearningRecallCard = (programId: string) => usePortabilityMutation<ReviewLearningRecallCardRequestDto, LearningRecallWorkspaceDto>(VaultAPI.reviewLearningRecallCard, programId);

export function useSearchLearningSourcesSemantically(request: SearchLearningSourcesSemanticallyRequestDto | null) {
  return useQuery<LearningSourceSemanticSearchResultDto[]>({
    queryKey: semanticSourcesKey(request),
    queryFn: async () => unwrap(await VaultAPI.searchLearningSourcesSemantically(request!)),
    enabled: Boolean(request?.programId && request.query.trim()), retry: false, staleTime: 5_000,
  });
}
