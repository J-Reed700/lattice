import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import { describeFailure } from '@/lib/errorHandling';
import type { HfTokenStatus } from '@/types/api/credentials';

export const HF_TOKEN_QUERY_KEY = ['huggingface', 'token-status'] as const;

export function useHuggingFaceTokenStatusQuery() {
  return useQuery<HfTokenStatus>({
    queryKey: HF_TOKEN_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getHuggingFaceTokenStatus();
      if (!result.ok) {
        throw new Error(describeFailure(result));
      }
      return result.data;
    },
    staleTime: 60_000,
    retry: false,
  });
}

export function useSetHuggingFaceTokenMutation() {
  const queryClient = useQueryClient();

  return useMutation<void, Error, string>({
    mutationFn: async (token) => {
      const result = await VaultAPI.setHuggingFaceToken(token);
      if (!result.ok) {
        throw new Error(describeFailure(result));
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: HF_TOKEN_QUERY_KEY });
    },
    retry: false,
  });
}

export function useDeleteHuggingFaceTokenMutation() {
  const queryClient = useQueryClient();

  return useMutation<void, Error, void>({
    mutationFn: async () => {
      const result = await VaultAPI.deleteHuggingFaceToken();
      if (!result.ok) {
        throw new Error(describeFailure(result));
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: HF_TOKEN_QUERY_KEY });
    },
    retry: false,
  });
}
