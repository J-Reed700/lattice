import { useRef, useState } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type {
  LearningProgramDto,
  LearningOutlineProgressDto,
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
  const activeRequest = useRef<string | null>(null);
  const [progress, setProgress] = useState<LearningOutlineProgressDto | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [cancelError, setCancelError] = useState<string>();
  const mutation = useMutation({
    // Program generation has no idempotency key, so recovery must be an
    // intentional user action rather than a blind mutation retry.
    retry: false,
    mutationFn: async (request: GenerateLearningProgramRequestDto) => {
      const requestId = crypto.randomUUID();
      activeRequest.current = requestId;
      setProgress(null);
      setCancelling(false);
      setCancelError(undefined);
      try {
        return unwrap(await VaultAPI.generateLearningProgram(request, { requestId, onProgress: (update) => {
          if (activeRequest.current === requestId) setProgress(update);
        } }));
      } finally {
        if (activeRequest.current === requestId) { activeRequest.current = null; setCancelling(false); }
      }
    },
    onSuccess: async (program) => {
      client.setQueryData(learningProgramKey(program.summary.id), program);
      await client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
    },
  });
  const cancel = async () => {
    const requestId = activeRequest.current;
    if (!requestId || cancelling) return;
    setCancelling(true);
    setCancelError(undefined);
    try {
      const accepted = unwrap(await VaultAPI.cancelLearningOutline(requestId));
      if (!accepted && activeRequest.current === requestId) setCancelling(false);
    } catch (error) {
      if (activeRequest.current === requestId) {
        setCancelling(false);
        setCancelError(error instanceof Error ? error.message : 'Could not cancel. Try again.');
      }
    }
  };
  return { ...mutation, progress, cancelling, cancelError, cancel };
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
