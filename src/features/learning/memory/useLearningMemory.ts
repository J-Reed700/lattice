import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningMemoryKey } from '@/features/learning/api/learningQueryKeys';
import type {
  EnsureLearningLessonNoteRequestDto,
  GenerateLearningCardDraftsRequestDto,
  LearningCardDraftActionRequestDto,
  LearningMemoryDto,
  ReviewStudyCardRequestDto,
  SaveLearningCardDraftRequestDto,
  UpdateStudyCardRequestDto,
} from '@/lib/bindings';

export { learningMemoryKey } from '@/features/learning/api/learningQueryKeys';

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningMemory(programId: string, enabled = true) {
  const api = useApi();
  return useQuery<LearningMemoryDto>({
    queryKey: learningMemoryKey(programId),
    queryFn: async () => unwrap(await api.getLearningMemory(programId)),
    enabled: enabled && Boolean(programId),
    staleTime: 10_000,
  });
}

function useMemoryMutation<TRequest>(mutation: (request: TRequest) => Promise<{ ok: true; data: LearningMemoryDto } | { ok: false; error: string }>) {
  const client = useQueryClient();
  return useMutation({
    // Several memory commands create durable records without an operation ID.
    // An automatic retry after a lost response could duplicate those records;
    // the panels expose an explicit retry with the original local input.
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutation(request)),
    onSuccess: async (memory) => {
      client.setQueryData(learningMemoryKey(memory.programId), memory);
    },
  });
}

export function useEnsureLearningLessonNote() {
  const api = useApi();
  return useMemoryMutation<EnsureLearningLessonNoteRequestDto>(api.ensureLearningLessonNote);
}
export function useGenerateLearningCardDrafts() {
  const api = useApi();
  return useMemoryMutation<GenerateLearningCardDraftsRequestDto>(api.generateLearningCardDrafts);
}
export function useSaveLearningCardDraft() {
  const api = useApi();
  return useMemoryMutation<SaveLearningCardDraftRequestDto>(api.saveLearningCardDraft);
}
export function useAcceptLearningCardDraft() {
  const api = useApi();
  return useMemoryMutation<LearningCardDraftActionRequestDto>(api.acceptLearningCardDraft);
}
export function useDiscardLearningCardDraft() {
  const api = useApi();
  return useMemoryMutation<LearningCardDraftActionRequestDto>(api.discardLearningCardDraft);
}

export function useUpdateLearningStudyCard() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (request: UpdateStudyCardRequestDto) => unwrap(await api.updateStudyCard(request)),
    onSuccess: async (_data, _request, _context, _mutationContext) => {
      await client.invalidateQueries({ queryKey: ['learning-memory'] });
    },
  });
}

export function useReviewLearningStudyCard() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (request: ReviewStudyCardRequestDto) => unwrap(await api.reviewStudyCard(request)),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['learning-memory'] });
    },
  });
}
