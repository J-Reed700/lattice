import { useCallback, useEffect, useMemo, useRef } from 'react';

import { type QueryClient, skipToken, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { LEARNING_PROGRAM_ROOT, LEARNING_PROGRAMS_KEY, learningCoursePlanKey, learningPlanKey, learningProgramKey } from '@/features/learning/api/learningQueryKeys';
import type VaultAPI from '@/lib/api';
import type {
  LearningCourseDepth,
  LearningOutlineRunsSnapshotDto,
  LearningProgramDto,
  LearningOutlineProgressDto,
  OutlineOperationKind,
  RepairLearningOutlineRequestDto,
  LearningProgramSummaryDto,
  GenerateLearningProgramRequestDto,
  AcceptLearningProgramRequestDto,
  PrepareLearningAheadRequestDto,
  PrepareLearningLessonRequestDto,
  PrepareLearningModuleRequestDto,
  CompleteLearningLessonRequestDto,
} from '@/lib/bindings';
import { failureToError } from '@/lib/errorHandling';
import type { ApiFailure } from '@/types/api/result';


export { LEARNING_PROGRAMS_KEY, learningProgramKey } from '@/features/learning/api/learningQueryKeys';

const GENERATE_OPERATION_KEY = ['learning-outline-operation', 'generate'] as const;
const LEARNING_OUTLINE_RUNS_KEY = ['learning-outline-runs'] as const;
const acknowledgedOutcomes = new WeakMap<QueryClient, Set<string>>();

function getAcknowledgedOutcomes(client: QueryClient): Set<string> {
  let set = acknowledgedOutcomes.get(client);
  if (!set) {
    set = new Set<string>();
    acknowledgedOutcomes.set(client, set);
  }
  return set;
}

function unwrap<T>(result: { ok: true; data: T } | ApiFailure): T {
  if (!result.ok) throw failureToError(result);
  return result.data;
}

export function useLearningPrograms() {
  const api = useApi();
  return useQuery<LearningProgramSummaryDto[]>({
    queryKey: LEARNING_PROGRAMS_KEY,
    queryFn: async () => unwrap(await api.listLearningPrograms()),
  });
}

export function useLearningProgram(id: string | null) {
  const api = useApi();
  return useQuery<LearningProgramDto>({
    queryKey: learningProgramKey(id ?? ''),
    queryFn: id ? async () => unwrap(await api.getLearningProgram(id)) : skipToken,
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
  const api = useApi();
  return useOutlineOperation(
    GENERATE_OPERATION_KEY,
    (request: GenerateLearningProgramRequestDto, options) => api.generateLearningProgram(request, options),
    {
      kind: 'generate',
      getMeta: (request) => ({ goal: request.goal, courseDepth: request.courseDepth }),
    },
  );
}
export function useRepairLearningOutline(programId: string) {
  const api = useApi();
  const key = useMemo(() => ['learning-outline-operation', 'repair', programId] as const, [programId]);
  return useOutlineOperation(
    key,
    (request: RepairLearningOutlineRequestDto, options) => api.repairLearningOutline(request, options),
    { kind: 'repair', programId },
  );
}
export type OutlineOperationState = {
  requestId: string | null;
  isPending: boolean;
  progress: LearningOutlineProgressDto | null;
  cancelling: boolean;
  cancelError?: string;
  error: Error | null;
  finishedProgramId: string | null;
  goal: string | null;
  courseDepth: LearningCourseDepth | null;
  builderFocused: boolean;
  hasLiveChannel: boolean;
};
const idleOutlineOperation: OutlineOperationState = {
  requestId: null,
  isPending: false,
  progress: null,
  cancelling: false,
  error: null,
  finishedProgramId: null,
  goal: null,
  courseDepth: null,
  builderFocused: false,
  hasLiveChannel: false,
};

function useOutlineOperation<T>(
  key: readonly string[],
  run: (request: T, options: { requestId: string; onProgress: (update: LearningOutlineProgressDto) => void }) => ReturnType<typeof VaultAPI.getLearningProgram>,
  descriptor: {
    kind: OutlineOperationKind;
    programId?: string;
    getMeta?: (request: T) => { goal?: string | null; courseDepth?: LearningCourseDepth | null };
  },
) {
  const api = useApi();
  const client = useQueryClient();
  const { kind, programId, getMeta } = descriptor;
  const lastHydratedProgramIdRef = useRef<string | null>(null);
  // IPC outlives the panel that started it. Keep its observable state with the
  // backend cache so reopening the draft rejoins the same request and cancel ID.
  const { data: operation = idleOutlineOperation } = useQuery<OutlineOperationState>({
    queryKey: key, queryFn: skipToken, initialData: idleOutlineOperation, gcTime: Infinity,
  });
  const canListRuns = typeof api.listLearningOutlineRuns === 'function';
  const runsQuery = useQuery<LearningOutlineRunsSnapshotDto | null>({
    queryKey: LEARNING_OUTLINE_RUNS_KEY,
    queryFn: canListRuns ? async () => unwrap(await api.listLearningOutlineRuns()) : skipToken,
    staleTime: 0,
    refetchOnMount: 'always',
    retry: false,
    refetchInterval: operation.isPending && !operation.hasLiveChannel ? 1500 : false,
    refetchOnWindowFocus: false,
  });
  const planTopics = useCallback(async (targetProgramId: string) => {
    try {
      client.setQueryData(learningCoursePlanKey(targetProgramId), unwrap(await api.planLearningCourse({ programId: targetProgramId })));
    } catch {
      await client.invalidateQueries({ queryKey: learningCoursePlanKey(targetProgramId) });
    }
  }, [api, client]);
  const updateOperation = useCallback((id: string, update: Partial<OutlineOperationState>) => {
    client.setQueryData<OutlineOperationState>(key, (current) => current?.requestId === id ? { ...current, ...update } : current);
  }, [client, key]);

  const snapshot = runsQuery.data;
  useEffect(() => {
    if (!snapshot) return;
    const current = client.getQueryData<OutlineOperationState>(key);
    const matchingRun = snapshot.active.find((runEntry) =>
      kind === 'generate'
        ? runEntry.kind === 'generate'
        : runEntry.kind === 'repair' && runEntry.progress.programId === programId,
    );

    if (matchingRun) {
      if (current?.hasLiveChannel && current.requestId === matchingRun.requestId) return;
      const sameRun = current?.requestId === matchingRun.requestId;
      client.setQueryData<OutlineOperationState>(key, {
        requestId: matchingRun.requestId,
        isPending: true,
        progress: matchingRun.progress,
        cancelling: sameRun ? current.cancelling : false,
        cancelError: sameRun ? current.cancelError : undefined,
        error: null,
        finishedProgramId: null,
        goal: matchingRun.goal ?? current?.goal ?? null,
        courseDepth: matchingRun.courseDepth ?? current?.courseDepth ?? null,
        builderFocused: kind === 'generate' ? (sameRun ? current.builderFocused : true) : false,
        hasLiveChannel: false,
      });
      if (matchingRun.progress.programId && lastHydratedProgramIdRef.current !== matchingRun.progress.programId) {
        lastHydratedProgramIdRef.current = matchingRun.progress.programId;
        void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
        void client.invalidateQueries({ queryKey: learningProgramKey(matchingRun.progress.programId) });
      }
      return;
    }

    if (current?.isPending && !current.hasLiveChannel && current.requestId) {
      const finishedId = current.requestId;
      getAcknowledgedOutcomes(client).add(finishedId);
      lastHydratedProgramIdRef.current = null;
      const outcome = snapshot.recentOutcomes.find((item) => item.requestId === finishedId);
      if (outcome?.error) {
        const converted = failureToError({
          ok: false,
          error: outcome.error.message,
          details: outcome.error,
        });
        updateOperation(finishedId, {
          requestId: null,
          isPending: false,
          cancelling: false,
          hasLiveChannel: false,
          error: converted,
        });
      } else if (outcome && !outcome.cancelled && outcome.programId) {
        const nextProgramId = outcome.programId;
        void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
        void client.invalidateQueries({ queryKey: learningProgramKey(nextProgramId) });
        void planTopics(nextProgramId);
        updateOperation(finishedId, {
          requestId: null,
          isPending: false,
          cancelling: false,
          hasLiveChannel: false,
          builderFocused: false,
          finishedProgramId: nextProgramId,
        });
      } else {
        void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
        updateOperation(finishedId, {
          requestId: null,
          isPending: false,
          cancelling: false,
          hasLiveChannel: false,
        });
      }
      return;
    }

    const currentError = current?.error ?? null;
    const currentFinishedId = current?.finishedProgramId ?? null;
    if (!current?.isPending && currentError === null && currentFinishedId === null) {
      const acknowledged = getAcknowledgedOutcomes(client);
      const unacknowledgedFailure = [...snapshot.recentOutcomes]
        .reverse()
        .find(
          (outcome) =>
            (kind === 'generate'
              ? outcome.kind === 'generate'
              : outcome.kind === 'repair' && outcome.programId === programId)
            && !outcome.cancelled
            && Boolean(outcome.error)
            && !acknowledged.has(outcome.requestId),
        );
      if (unacknowledgedFailure?.error) {
        acknowledged.add(unacknowledgedFailure.requestId);
        const converted = failureToError({
          ok: false,
          error: unacknowledgedFailure.error.message,
          details: unacknowledgedFailure.error,
        });
        client.setQueryData<OutlineOperationState>(key, {
          ...(current ?? idleOutlineOperation),
          requestId: null,
          isPending: false,
          cancelling: false,
          hasLiveChannel: false,
          builderFocused: kind === 'generate',
          error: converted,
        });
      }
    }
  }, [client, key, kind, planTopics, programId, snapshot, updateOperation]);

  const mutation = useMutation({
    // Resume is an explicit action against the saved draft revision.
    retry: false,
    mutationFn: async (request: T) => {
      if (client.getQueryData<OutlineOperationState>(key)?.isPending) throw new Error('This outline operation is already running.');
      const requestId = crypto.randomUUID();
      let firstSeenProgramId: string | null = null;
      let lastRefresh = 0;
      const meta = getMeta?.(request);
      client.setQueryData<OutlineOperationState>(key, {
        ...idleOutlineOperation,
        requestId,
        isPending: true,
        goal: meta?.goal ?? null,
        courseDepth: meta?.courseDepth ?? null,
        builderFocused: kind === 'generate',
        hasLiveChannel: true,
      });
      try {
        const program = unwrap(await run(request, { requestId, onProgress: (update) => {
          if (client.getQueryData<OutlineOperationState>(key)?.requestId !== requestId) return;
          updateOperation(requestId, { progress: update });
          if (update.programId) {
            const now = Date.now();
            if (firstSeenProgramId !== update.programId) {
              firstSeenProgramId = update.programId;
              lastRefresh = now;
              void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
              void client.invalidateQueries({ queryKey: learningProgramKey(update.programId) });
            } else if (now - lastRefresh > 5000) {
              lastRefresh = now;
              void client.invalidateQueries({ queryKey: learningProgramKey(update.programId) });
              void client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
            }
          }
        } }));
        getAcknowledgedOutcomes(client).add(requestId);
        client.setQueryData(learningProgramKey(program.summary.id), program);
        updateOperation(requestId, {
          requestId: null,
          isPending: false,
          cancelling: false,
          finishedProgramId: program.summary.id,
          builderFocused: false,
          hasLiveChannel: false,
        });
        return program;
      } catch (error) {
        getAcknowledgedOutcomes(client).add(requestId);
        updateOperation(requestId, {
          requestId: null,
          isPending: false,
          cancelling: false,
          hasLiveChannel: false,
          error: error instanceof Error ? error : new Error(String(error)),
        });
        throw error;
      }
    },
    onSuccess: (program) => { void planTopics(program.summary.id); },
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY });
      await client.invalidateQueries({ queryKey: LEARNING_PROGRAM_ROOT });
    },
  });
  const cancel = useCallback(async () => {
    const current = client.getQueryData<OutlineOperationState>(key);
    const requestId = current?.requestId;
    if (!requestId || current.cancelling) return;
    updateOperation(requestId, { cancelling: true, cancelError: undefined });
    try {
      const accepted = unwrap(await api.cancelLearningOutline(requestId));
      if (!accepted) {
        updateOperation(requestId, { cancelling: false });
        if (canListRuns && !current.hasLiveChannel) {
          void client.invalidateQueries({ queryKey: LEARNING_OUTLINE_RUNS_KEY });
        }
      }
    } catch (error) {
      updateOperation(requestId, { cancelling: false, cancelError: error instanceof Error ? error.message : 'Could not cancel. Try again.' });
    }
  }, [api, canListRuns, client, key, updateOperation]);
  const resetMutation = mutation.reset;
  const reset = useCallback(() => {
    resetMutation();
    if (!client.getQueryData<OutlineOperationState>(key)?.isPending) client.setQueryData(key, idleOutlineOperation);
  }, [client, key, resetMutation]);
  const setBuilderFocused = useCallback((focused: boolean) => {
    client.setQueryData<OutlineOperationState>(key, (current) => {
      const base = current ?? idleOutlineOperation;
      return base.builderFocused === focused ? base : { ...base, builderFocused: focused };
    });
  }, [client, key]);
  const clearFinished = useCallback(() => {
    client.setQueryData<OutlineOperationState>(key, (current) => {
      if (!current || (current.finishedProgramId === null && !current.builderFocused)) return current;
      return { ...current, finishedProgramId: null, builderFocused: false };
    });
  }, [client, key]);
  const dismissError = useCallback(() => {
    client.setQueryData<OutlineOperationState>(key, (current) => {
      if (!current || current.isPending || current.error === null) return current;
      return { ...current, error: null };
    });
  }, [client, key]);
  return { ...mutation, ...operation, cancel, reset, setBuilderFocused, clearFinished, dismissError };
}

export function useAcceptLearningProgram() {
  const api = useApi();
  return useProgramMutation<AcceptLearningProgramRequestDto>(api.acceptLearningProgram);
}
export function usePrepareLearningLesson() {
  const api = useApi();
  return useProgramMutation<PrepareLearningLessonRequestDto>(api.prepareLearningLesson);
}
export function usePrepareLearningModule() {
  const api = useApi();
  return useProgramMutation<PrepareLearningModuleRequestDto>(api.prepareLearningModule);
}
export function usePrepareLearningAhead() {
  const api = useApi();
  return useProgramMutation<PrepareLearningAheadRequestDto>(api.prepareLearningAhead);
}
export function useCompleteLearningLesson() {
  const api = useApi();
  return useProgramMutation<CompleteLearningLessonRequestDto>(api.completeLearningLesson);
}
