import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type {
  EnsureLearningLessonNoteRequestDto,
  GenerateLearningCardDraftsRequestDto,
  LearningCardDraftActionRequestDto,
  LearningMemoryDto,
  ReviewStudyCardRequestDto,
  SaveLearningCardDraftRequestDto,
  UpdateStudyCardRequestDto,
} from '@/lib/bindings';

export const learningMemoryKey = (programId: string) => ['learning-memory', programId] as const;

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningMemory(programId: string, enabled = true) {
  return useQuery<LearningMemoryDto>({
    queryKey: learningMemoryKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningMemory(programId)),
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
  return useMemoryMutation<EnsureLearningLessonNoteRequestDto>(VaultAPI.ensureLearningLessonNote);
}
export function useGenerateLearningCardDrafts() {
  return useMemoryMutation<GenerateLearningCardDraftsRequestDto>(VaultAPI.generateLearningCardDrafts);
}
export function useSaveLearningCardDraft() {
  return useMemoryMutation<SaveLearningCardDraftRequestDto>(VaultAPI.saveLearningCardDraft);
}
export function useAcceptLearningCardDraft() {
  return useMemoryMutation<LearningCardDraftActionRequestDto>(VaultAPI.acceptLearningCardDraft);
}
export function useDiscardLearningCardDraft() {
  return useMemoryMutation<LearningCardDraftActionRequestDto>(VaultAPI.discardLearningCardDraft);
}

export function useUpdateLearningStudyCard() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (request: UpdateStudyCardRequestDto) => unwrap(await VaultAPI.updateStudyCard(request)),
    onSuccess: async (_data, _request, _context, _mutationContext) => {
      await client.invalidateQueries({ queryKey: ['learning-memory'] });
    },
  });
}

export function useReviewLearningStudyCard() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (request: ReviewStudyCardRequestDto) => unwrap(await VaultAPI.reviewStudyCard(request)),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['learning-memory'] });
    },
  });
}
