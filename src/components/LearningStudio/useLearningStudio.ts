import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type {
  LearningProgramDto,
  LearningProgramSummaryDto,
  GenerateLearningProgramRequestDto,
  AcceptLearningProgramRequestDto,
  PrepareLearningLessonRequestDto,
  CompleteLearningLessonRequestDto,
  SubmitLearningAttemptRequestDto,
} from '@/lib/bindings';

export const LEARNING_PROGRAMS_KEY = ['learning-programs'] as const;
export const learningProgramKey = (id: string) => ['learning-program', id] as const;

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningPrograms() {
  return useQuery<LearningProgramSummaryDto[]>({
    queryKey: LEARNING_PROGRAMS_KEY,
    queryFn: async () => unwrap(await VaultAPI.listLearningPrograms()),
  });
}

export function useLearningProgram(id: string | null) {
  return useQuery<LearningProgramDto>({
    queryKey: id ? learningProgramKey(id) : ['learning-program', 'none'],
    enabled: Boolean(id),
    queryFn: async () => unwrap(await VaultAPI.getLearningProgram(id!)),
  });
}

function useProgramMutation<TRequest>(mutate: (request: TRequest) => ReturnType<typeof VaultAPI.getLearningProgram>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutate(request)),
    onSuccess: async (program) => {
      client.setQueryData(learningProgramKey(program.summary.id), program);
      await client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
    },
  });
}

export function useGenerateLearningProgram() {
  const client = useQueryClient();
  return useMutation({
    // Program generation has no idempotency key, so recovery must be an
    // intentional user action rather than a blind mutation retry.
    retry: false,
    mutationFn: async (request: GenerateLearningProgramRequestDto) => unwrap(await VaultAPI.generateLearningProgram(request)),
    onSuccess: async (program) => {
      client.setQueryData(learningProgramKey(program.summary.id), program);
      await client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
    },
  });
}

export function useAcceptLearningProgram() {
  return useProgramMutation<AcceptLearningProgramRequestDto>(VaultAPI.acceptLearningProgram);
}
export function usePrepareLearningLesson() {
  return useProgramMutation<PrepareLearningLessonRequestDto>(VaultAPI.prepareLearningLesson);
}
export function useCompleteLearningLesson() {
  return useProgramMutation<CompleteLearningLessonRequestDto>(VaultAPI.completeLearningLesson);
}
export function useSubmitLearningAttempt() {
  return useProgramMutation<SubmitLearningAttemptRequestDto>(VaultAPI.submitLearningAttempt);
}
