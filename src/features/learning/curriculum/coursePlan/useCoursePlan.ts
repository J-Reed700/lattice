import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningCoursePlanKey } from '@/features/learning/api/learningQueryKeys';
import type { CoursePlanViewDto } from '@/lib/bindings';
import { failureToError } from '@/lib/errorHandling';
import type { ApiFailure } from '@/types/api/result';

const POLL_MS = 2_000;

function unwrap<T>(result: { ok: true; data: T } | ApiFailure): T {
  if (!result.ok) throw failureToError(result);
  return result.data;
}

export function useCoursePlan(programId: string) {
  const api = useApi();
  return useQuery<CoursePlanViewDto>({
    queryKey: learningCoursePlanKey(programId),
    queryFn: async () => unwrap(await api.getLearningCoursePlan(programId)),
    enabled: Boolean(programId),
    refetchInterval: (query) => (query.state.data?.state === 'planning' ? POLL_MS : false),
    refetchIntervalInBackground: true,
    retry: false,
  });
}

function usePlanMutation<TRequest extends { programId: string }>(run: (request: TRequest) => Promise<{ ok: true; data: CoursePlanViewDto } | ApiFailure>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await run(request)),
    onSuccess: (view) => {
      client.setQueryData(learningCoursePlanKey(view.programId), view);
    },
    onError: (_error, request) => {
      void client.invalidateQueries({ queryKey: learningCoursePlanKey(request.programId) });
    },
  });
}

export function usePlanCourse() {
  const api = useApi();
  return usePlanMutation(api.planLearningCourse);
}

export function useEditCoursePlanEntry() {
  const api = useApi();
  return usePlanMutation(api.editLearningCoursePlanEntry);
}

export function useChangeCoursePlanEntries() {
  const api = useApi();
  return usePlanMutation(api.changeLearningCoursePlanEntries);
}

export function useUndoCoursePlanEntry() {
  const api = useApi();
  return usePlanMutation(api.undoLearningCoursePlanEntry);
}
