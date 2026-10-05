import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { LEARNING_PROGRAMS_KEY, learningPlanKey, learningProgramKey } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type {
  LearningProgramDto,
  LearningOutlineProgressDto,
  RepairLearningOutlineRequestDto,
  LearningProgramSummaryDto,
  GenerateLearningProgramRequestDto,
  AcceptLearningProgramRequestDto,
  PrepareLearningLessonRequestDto,
  CompleteLearningLessonRequestDto,
  SubmitLearningAttemptRequestDto,
} from '@/lib/bindings';


export { LEARNING_PROGRAMS_KEY, learningProgramKey } from '@/features/learning/api/learningQueryKeys';

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
      await Promise.all([
        client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY }),
        client.invalidateQueries({ queryKey: learningPlanKey(program.summary.id) }),
      ]);
    },
  });
}

export function useGenerateLearningProgram() {
  return useOutlineOperation(['learning-outline-operation', 'generate'], (request: GenerateLearningProgramRequestDto, options) => VaultAPI.generateLearningProgram(request, options));
}
export function useRepairLearningOutline(programId: string) {
  return useOutlineOperation(['learning-outline-operation', 'repair', programId], (request: RepairLearningOutlineRequestDto, options) => VaultAPI.repairLearningOutline(request, options));
}
type OutlineOperationState = {
  requestId: string | null;
  isPending: boolean;
  progress: LearningOutlineProgressDto | null;
  cancelling: boolean;
  cancelError?: string;
  error: Error | null;
};
const idleOutlineOperation: OutlineOperationState = { requestId: null, isPending: false, progress: null, cancelling: false, error: null };

function useOutlineOperation<T>(key: readonly string[], run: (request: T, options: { requestId: string; onProgress: (update: LearningOutlineProgressDto) => void }) => ReturnType<typeof VaultAPI.getLearningProgram>) {
  const client = useQueryClient();
  // IPC outlives the panel that started it. Keep its observable state with the
  // backend cache so reopening the draft rejoins the same request and cancel ID.
  const { data: operation } = useQuery<OutlineOperationState>({
    queryKey: key, enabled: false, initialData: idleOutlineOperation, gcTime: Infinity,
  });
  const updateOperation = (id: string, update: Partial<OutlineOperationState>) => {
    client.setQueryData<OutlineOperationState>(key, (current) => current?.requestId === id ? { ...current, ...update } : current);
  };
  const mutation = useMutation({
    // Resume is an explicit action against the saved draft revision.
    retry: false,
    mutationFn: async (request: T) => {
      if (client.getQueryData<OutlineOperationState>(key)?.isPending) throw new Error('This outline operation is already running.');
      const requestId = crypto.randomUUID();
      let lastRefresh = 0;
      client.setQueryData<OutlineOperationState>(key, { ...idleOutlineOperation, requestId, isPending: true });
      try {
        return unwrap(await run(request, { requestId, onProgress: (update) => {
          if (client.getQueryData<OutlineOperationState>(key)?.requestId !== requestId) return;
          updateOperation(requestId, { progress: update });
          if (update.programId && Date.now() - lastRefresh > 5000) {
            lastRefresh = Date.now();
            void client.invalidateQueries({ queryKey: learningProgramKey(update.programId) });
          }
        } }));
      } catch (error) {
        updateOperation(requestId, { error: error instanceof Error ? error : new Error(String(error)) });
        throw error;
      } finally {
        updateOperation(requestId, { requestId: null, isPending: false, cancelling: false });
      }
    },
    onSuccess: async (program) => {
      client.setQueryData(learningProgramKey(program.summary.id), program);
      await client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
    },
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
      await client.invalidateQueries({ queryKey: ['learning-program'] });
    },
  });
  const cancel = async () => {
    const current = client.getQueryData<OutlineOperationState>(key);
    const requestId = current?.requestId;
    if (!requestId || current.cancelling) return;
    updateOperation(requestId, { cancelling: true, cancelError: undefined });
    try {
      const accepted = unwrap(await VaultAPI.cancelLearningOutline(requestId));
      if (!accepted) updateOperation(requestId, { cancelling: false });
    } catch (error) {
      updateOperation(requestId, { cancelling: false, cancelError: error instanceof Error ? error.message : 'Could not cancel. Try again.' });
    }
  };
  const reset = () => {
    mutation.reset();
    if (!client.getQueryData<OutlineOperationState>(key)?.isPending) client.setQueryData(key, idleOutlineOperation);
  };
  return { ...mutation, ...operation, cancel, reset };
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
