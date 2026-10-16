import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { learningApi } from '@/features/learning/api/client';
import { learningTangentsKey } from '@/features/learning/api/learningQueryKeys';
import type {
  CreateLearningTangentRequestDto,
  LearningTangentDto,
} from '@/lib/bindings';

export function useLearningTangents(programId?: string | null) {
  return useQuery<LearningTangentDto[]>({
    queryKey: learningTangentsKey(programId),
    queryFn: async () => (await learningApi.listLearningTangents(programId)) ?? [],
    staleTime: 10_000,
  });
}

export function useCreateLearningTangent() {
  const queryClient = useQueryClient();

  return useMutation<
    LearningTangentDto,
    Error,
    CreateLearningTangentRequestDto
  >({
    mutationFn: (request) => learningApi.createLearningTangent(request),
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
