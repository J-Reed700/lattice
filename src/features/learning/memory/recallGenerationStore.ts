import { create } from 'zustand';

import { learningMemoryKey } from '@/features/learning/api/learningQueryKeys';
import type { GenerateLearningCardDraftsRequestDto, LearningMemoryDto } from '@/lib/bindings';
import { unwrapApiResult as unwrap, type ApiResult } from '@/types/api/result';

import type { QueryClient } from '@tanstack/react-query';

export interface RecallGenerationRun {
  pending: boolean;
  programId: string;
  lessonId: string | null;
  error: string | null;
}

export const recallGenerationKey = (programId: string, lessonId?: string | null) =>
  `${programId}:${lessonId ?? 'course'}`;

interface RecallGenerationState {
  runs: Record<string, RecallGenerationRun>;
  startGeneration: (params: {
    request: GenerateLearningCardDraftsRequestDto;
    call: (request: GenerateLearningCardDraftsRequestDto) => Promise<ApiResult<LearningMemoryDto>>;
    queryClient: QueryClient;
  }) => Promise<LearningMemoryDto | null>;
  clearError: (programId: string, lessonId?: string | null) => void;
}

const initialState = () => ({ runs: {} as Record<string, RecallGenerationRun> });

/** Persists Recall draft card generation state and errors per lesson across tab/route switches. */
export const useRecallGenerationStore = create<RecallGenerationState>((set, get) => ({
  ...initialState(),
  startGeneration: async ({ request, call, queryClient }) => {
    const { programId, lessonId } = request;
    const key = recallGenerationKey(programId, lessonId);
    if (get().runs[key]?.pending) return null;
    set((state) => ({
      runs: {
        ...state.runs,
        [key]: { pending: true, programId, lessonId, error: null },
      },
    }));
    try {
      const memory = unwrap(await call(request));
      queryClient.setQueryData(learningMemoryKey(memory.programId), memory);
      set((state) => ({
        runs: {
          ...state.runs,
          [key]: { pending: false, programId, lessonId, error: null },
        },
      }));
      return memory;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'Draft generation failed';
      set((state) => ({
        runs: {
          ...state.runs,
          [key]: { pending: false, programId, lessonId, error: message },
        },
      }));
      return null;
    }
  },
  clearError: (programId, lessonId) => set((state) => {
    const key = recallGenerationKey(programId, lessonId);
    const existing = state.runs[key];
    if (!existing?.error) return state;
    return {
      runs: {
        ...state.runs,
        [key]: { ...existing, error: null },
      },
    };
  }),
}));

export function resetRecallGenerationStore(): void {
  useRecallGenerationStore.setState(initialState());
}
