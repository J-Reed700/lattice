import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningTangentsKey } from '@/features/learning/api/learningQueryKeys';
import type {
  CreateLearningTangentRequestDto,
  LearningTangentDto,
} from '@/lib/bindings';

export function useLearningTangents(programId?: string | null) {
  const api = useApi();
  return useQuery<LearningTangentDto[]>({
    queryKey: learningTangentsKey(programId),
    queryFn: async () => (await api.listLearningTangents(programId)) ?? [],
    staleTime: 10_000,
  });
}

export function useCreateLearningTangent() {
  const api = useApi();
  const queryClient = useQueryClient();

  return useMutation<
    LearningTangentDto,
    Error,
    CreateLearningTangentRequestDto
  >({
    mutationFn: (request) => api.createLearningTangent(request),
    onSuccess: (created) => {
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(created.programId),
      });
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(null),
      });
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(undefined),
      });
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(),
      });
    },
  });
}
