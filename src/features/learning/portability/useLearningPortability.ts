import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { invalidateLearningSourceDependents, refreshLearningProgramAfterImport } from '@/features/learning/api/learningInvalidation';
import { learningRecallKey } from '@/features/learning/api/learningQueryKeys';
import { CONFLICT_KINDS } from '@/features/learning/portability/conflictPolicies';
import VaultAPI from '@/lib/api';
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
  return useQuery<LearningPortabilityWorkspaceDto>({
    queryKey: learningPortabilityKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningPortabilityWorkspace(programId)),
    enabled: Boolean(programId), retry: false, staleTime: 3_000,
    // Nothing announces that a course finished preparing, so check while it is.
    refetchInterval: (query) => query.state.data?.exportNotice.blockers.some((kind) => CONFLICT_KINDS[kind].retryable) ? 5_000 : false,
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

export const useExportLearningPack = (programId: string) => usePortabilityMutation<ExportLearningPackRequestDto, LearningPortabilityWorkspaceDto>(VaultAPI.exportLearningPack, programId);
export const useSaveLearningPackAs = (programId: string) => usePortabilityMutation<ExportLearningPackRequestDto, SaveLearningPackAsResultDto>(VaultAPI.saveLearningPackAs, programId);

/** Import with no course open: preview, apply and cancel are not keyed by a program. */
export function useImportCourse() {
  const client = useQueryClient();
  const remember = (data: LearningPortabilityWorkspaceDto) => { client.setQueryData(learningPortabilityKey(data.programId), data); };
  const preview = useMutation({
    retry: false,
    mutationFn: async (request: PreviewLearningPackImportRequestDto) => unwrap(await VaultAPI.previewLearningPackImport(request)),
    onSuccess: remember,
  });
  const cancel = useMutation({
    retry: false,
    mutationFn: async (request: CancelLearningPackImportPreviewRequestDto) => unwrap(await VaultAPI.cancelLearningPackImportPreview(request)),
    onSuccess: remember,
  });
  const apply = useMutation({
    retry: false,
    mutationFn: async (request: ApplyLearningPackImportRequestDto) => {
      if (!await flushPendingSaves()) {
        throw new Error('Your work could not be saved before importing. Resolve the save error and retry the import.');
      }
      return unwrap(await VaultAPI.applyLearningPackImport(request));
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
  const client = useQueryClient();
  return useMutation({
    mutationKey: EXPORT_ALL_MUTATION_KEY,
    retry: false,
    mutationFn: async (request: ExportAllLearningPacksRequestDto) => unwrap(await VaultAPI.exportAllLearningPacks(request)),
    // Every exported course gains a row in its own export history.
    onSuccess: async (result) => {
      if (result.outcome === 'exported') await client.invalidateQueries({ queryKey: LEARNING_PORTABILITY_PREFIX });
    },
  });
}

/** True while an export of every course runs, wherever it was started; its results arrive only at the end. */
export const useExportAllPending = () => useIsMutating({ mutationKey: EXPORT_ALL_MUTATION_KEY }) > 0;
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
