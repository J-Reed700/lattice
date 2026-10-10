import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { invalidateLearningSourceDependents, refreshLearningProgramAfterImport } from '@/features/learning/api/learningInvalidation';
import { learningRecallKey } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type {
  ApplyLearningPackImportRequestDto,
  CancelLearningPackImportPreviewRequestDto,
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
import { flushPendingSaves } from '@/lib/pendingSaves';

export const learningPortabilityKey = (programId: string) => ['learning-portability', programId] as const;
export { learningRecallKey } from '@/features/learning/api/learningQueryKeys';
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
  replacesProgram = false,
) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => {
      if (replacesProgram && !await flushPendingSaves()) {
        throw new Error('Your work could not be saved before importing. Resolve the save error and retry the import.');
      }
      return unwrap(await mutation(request));
    },
    onSuccess: async (data) => {
      if (data && typeof data === 'object' && 'programId' in data && typeof data.programId === 'string'
        && (replacesProgram || 'sources' in data)) {
        if ('sources' in data) client.setQueryData(['learning-sources', data.programId], data);
        if (replacesProgram) await refreshLearningProgramAfterImport(client, data.programId);
        else await invalidateLearningSourceDependents(client, data.programId);
      }
      if (data && typeof data === 'object' && 'programId' in data && typeof data.programId === 'string') {
        if ('exports' in data) client.setQueryData(learningPortabilityKey(data.programId), data);
        if ('cards' in data) client.setQueryData(learningRecallKey(data.programId), data);
      }
      await client.invalidateQueries({ queryKey: learningPortabilityKey(programId) });
    },
  });
}

export const useExportLearningPack = (programId: string) => usePortabilityMutation<ExportLearningPackRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.exportLearningPack, programId);
export const usePreviewLearningPackImport = (programId: string) => usePortabilityMutation<PreviewLearningPackImportRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.previewLearningPackImport, programId);
export const useApplyLearningPackImport = (programId: string) => usePortabilityMutation<ApplyLearningPackImportRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.applyLearningPackImport, programId, true);
export const useCancelLearningPackImportPreview = (programId: string) => usePortabilityMutation<CancelLearningPackImportPreviewRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.cancelLearningPackImportPreview, programId);
export const useDeleteLearningSourceV2 = (programId: string) => usePortabilityMutation<DeleteLearningSourceRequestDto, LearningSourceWorkspaceDto>(VaultAPI.deleteLearningSource, programId);
export const useReimportLearningSource = (programId: string) => usePortabilityMutation<ReimportLearningSourceRequestDto, LearningSourceWorkspaceDto>(VaultAPI.reimportLearningSource, programId);
export const useCreateLearningSourceSelector = (programId: string) => usePortabilityMutation<CreateLearningSourceSelectorRequestDto, LearningSourceSelectorDto>(VaultAPI.createLearningSourceSelector, programId);
export const useSaveLearningRecallCard = (programId: string) => usePortabilityMutation<SaveLearningRecallCardRequestDto, LearningRecallWorkspaceDto>(VaultAPI.saveLearningRecallCard, programId);
export const useDecideLearningRecallDuplicate = (programId: string) => usePortabilityMutation<DecideLearningRecallDuplicateRequestDto, LearningRecallWorkspaceDto>(VaultAPI.decideLearningRecallDuplicate, programId);
export const useReviewLearningRecallCard = (programId: string) => usePortabilityMutation<ReviewLearningRecallCardRequestDto, LearningRecallWorkspaceDto>(VaultAPI.reviewLearningRecallCard, programId);

export function useSearchLearningSourcesSemantically(request: SearchLearningSourcesSemanticallyRequestDto | null) {
  return useQuery<LearningSourceSemanticSearchResultDto[]>({
    queryKey: semanticSourcesKey(request),
    queryFn: async () => unwrap(await VaultAPI.searchLearningSourcesSemantically(request!)),
    enabled: Boolean(request?.programId && request.query.trim()), retry: false, staleTime: 5_000,
  });
}
