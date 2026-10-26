import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { invalidateLearningSourceDependents, refreshLearningProgramAfterImport } from '@/features/learning/api/learningInvalidation';
import { learningRecallKey } from '@/features/learning/api/learningQueryKeys';
import { CONFLICT_KINDS } from '@/features/learning/portability/conflictPolicies';
import type {
  ApplyLearningPackImportRequestDto,
  CancelLearningPackImportPreviewRequestDto,
  CreateLearningSourceSelectorRequestDto,
  DecideLearningRecallDuplicateRequestDto,
  DeleteLearningSourceRequestDto,
  ExportAllLearningPacksRequestDto,
  ExportLearningPackRequestDto,
  LearningPortabilityWorkspaceDto,
  LearningRecallWorkspaceDto,
  LearningSourceSelectorDto,
  LearningSourceSemanticSearchResultDto,
  LearningSourceWorkspaceDto,
  PreviewLearningPackImportRequestDto,
  ReimportLearningSourceRequestDto,
  ReviewLearningRecallCardRequestDto,
  SaveLearningPackAsResultDto,
  SaveLearningRecallCardRequestDto,
  SearchLearningSourcesSemanticallyRequestDto,
} from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';

export const LEARNING_PORTABILITY_PREFIX = ['learning-portability'] as const;
export const learningPortabilityKey = (programId: string) => [...LEARNING_PORTABILITY_PREFIX, programId] as const;
export { learningRecallKey } from '@/features/learning/api/learningQueryKeys';
export const semanticSourcesKey = (request: SearchLearningSourcesSemanticallyRequestDto | null) =>
  ['learning-semantic-sources', request?.programId, request?.query, request?.limit] as const;

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningPortability(programId: string) {
  const api = useApi();
  return useQuery<LearningPortabilityWorkspaceDto>({
    queryKey: learningPortabilityKey(programId),
    queryFn: async () => unwrap(await api.getLearningPortabilityWorkspace(programId)),
    enabled: Boolean(programId), retry: false, staleTime: 3_000,
    // Nothing announces that a course finished preparing, so check while it is.
    refetchInterval: (query) => query.state.data?.exportNotice.blockers.some((kind) => CONFLICT_KINDS[kind].retryable) ? 5_000 : false,
  });
}

export function useLearningRecallV2(programId: string) {
  const api = useApi();
  return useQuery<LearningRecallWorkspaceDto>({
    queryKey: learningRecallKey(programId),
    queryFn: async () => unwrap(await api.getLearningRecallWorkspace(programId)),
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
      if (data && typeof data === 'object' && 'programId' in data && typeof data.programId === 'string') {
        if ('sources' in data) {
          client.setQueryData(['learning-sources', data.programId], data);
          await invalidateLearningSourceDependents(client, data.programId);
        }
        if ('exports' in data) client.setQueryData(learningPortabilityKey(data.programId), data);
        if ('cards' in data) client.setQueryData(learningRecallKey(data.programId), data);
      }
      await client.invalidateQueries({ queryKey: learningPortabilityKey(programId) });
    },
  });
}

export const useExportLearningPack = (programId: string) => {
  const api = useApi();
  return usePortabilityMutation<ExportLearningPackRequestDto, LearningPortabilityWorkspaceDto>(api.exportLearningPack, programId);
};
export const useSaveLearningPackAs = (programId: string) => {
  const api = useApi();
  return usePortabilityMutation<ExportLearningPackRequestDto, SaveLearningPackAsResultDto>(api.saveLearningPackAs, programId);
};

/** Import with no course open: preview, apply and cancel are not keyed by a program. */
export function useImportCourse() {
  const api = useApi();
  const client = useQueryClient();
  const remember = (data: LearningPortabilityWorkspaceDto) => { client.setQueryData(learningPortabilityKey(data.programId), data); };
  const preview = useMutation({
    retry: false,
    mutationFn: async (request: PreviewLearningPackImportRequestDto) => unwrap(await api.previewLearningPackImport(request)),
    onSuccess: remember,
  });
  const cancel = useMutation({
    retry: false,
    mutationFn: async (request: CancelLearningPackImportPreviewRequestDto) => unwrap(await api.cancelLearningPackImportPreview(request)),
    onSuccess: remember,
  });
  const apply = useMutation({
    retry: false,
    mutationFn: async (request: ApplyLearningPackImportRequestDto) => {
      if (!await flushPendingSaves()) {
        throw new Error('Your work could not be saved before importing. Resolve the save error and retry the import.');
      }
      return unwrap(await api.applyLearningPackImport(request));
    },
    // The response's programId is the imported course. The refresh also
    // invalidates LEARNING_PROGRAMS_KEY and that course's detail query.
    onSuccess: async (data) => {
      remember(data);
      await refreshLearningProgramAfterImport(client, data.programId);
    },
  });
  return { preview, apply, cancel };
}

const EXPORT_ALL_MUTATION_KEY = ['learning-export-all'] as const;

export function useExportAllLearningPacks() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationKey: EXPORT_ALL_MUTATION_KEY,
    retry: false,
    mutationFn: async (request: ExportAllLearningPacksRequestDto) => unwrap(await api.exportAllLearningPacks(request)),
    // Every exported course gains a row in its own export history.
    onSuccess: async (result) => {
      if (result.outcome === 'exported') await client.invalidateQueries({ queryKey: LEARNING_PORTABILITY_PREFIX });
    },
  });
}

/** True while an export of every course runs, wherever it was started; its results arrive only at the end. */
export const useExportAllPending = () => useIsMutating({ mutationKey: EXPORT_ALL_MUTATION_KEY }) > 0;
export const useDeleteLearningSourceV2 = (programId: string) => {
  const api = useApi();
  return usePortabilityMutation<DeleteLearningSourceRequestDto, LearningSourceWorkspaceDto>(api.deleteLearningSource, programId);
};
export const useReimportLearningSource = (programId: string) => {
  const api = useApi();
  return usePortabilityMutation<ReimportLearningSourceRequestDto, LearningSourceWorkspaceDto>(api.reimportLearningSource, programId);
};
export const useCreateLearningSourceSelector = (programId: string) => {
  const api = useApi();
  return usePortabilityMutation<CreateLearningSourceSelectorRequestDto, LearningSourceSelectorDto>(api.createLearningSourceSelector, programId);
};
export const useSaveLearningRecallCard = (programId: string) => {
  const api = useApi();
  return usePortabilityMutation<SaveLearningRecallCardRequestDto, LearningRecallWorkspaceDto>(api.saveLearningRecallCard, programId);
};
export const useDecideLearningRecallDuplicate = (programId: string) => {
  const api = useApi();
  return usePortabilityMutation<DecideLearningRecallDuplicateRequestDto, LearningRecallWorkspaceDto>(api.decideLearningRecallDuplicate, programId);
};
export const useReviewLearningRecallCard = (programId: string) => {
  const api = useApi();
  return usePortabilityMutation<ReviewLearningRecallCardRequestDto, LearningRecallWorkspaceDto>(api.reviewLearningRecallCard, programId);
};

export function useSearchLearningSourcesSemantically(request: SearchLearningSourcesSemanticallyRequestDto | null) {
  const api = useApi();
  return useQuery<LearningSourceSemanticSearchResultDto[]>({
    queryKey: semanticSourcesKey(request),
    queryFn: async () => unwrap(await api.searchLearningSourcesSemantically(request!)),
    enabled: Boolean(request?.programId && request.query.trim()), retry: false, staleTime: 5_000,
  });
}
