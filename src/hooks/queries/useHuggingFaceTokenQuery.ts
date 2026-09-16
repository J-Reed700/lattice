import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { HfTokenStatus } from '@/types/api/credentials';

export const HF_TOKEN_QUERY_KEY = ['huggingface', 'token-status'] as const;

/**
 * Whether a Hugging Face token is stored — nothing more.
 *
 * Only `{ isSet }` ever enters the cache. The token itself lives in the input
 * the user is typing into and is never read back from the backend: the
 * `get_huggingface_token` command exists but must not be called from the UI.
 */
export function useHuggingFaceTokenStatusQuery() {
  return useQuery<HfTokenStatus>({
    queryKey: HF_TOKEN_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getHuggingFaceTokenStatus();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 60_000,
    // A rejected OS keyring request cannot succeed on an automatic retry.
    // Retrying would show the same native permission dialog repeatedly.
    retry: false,
  });
}

/**
 * Stores a token. The plaintext is a call argument and nothing else — it is
 * never cached, logged, or put in a toast. Call `reset()` after a success so
 * React Query drops the mutation variables too.
 */
export function useSetHuggingFaceTokenMutation() {
  const queryClient = useQueryClient();

  return useMutation<void, Error, string>({
    mutationFn: async (token) => {
      const result = await VaultAPI.setHuggingFaceToken(token);
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: HF_TOKEN_QUERY_KEY });
    },
    // Secure-storage writes require an explicit user action and must not be
    // replayed automatically after a Keychain denial.
    retry: false,
  });
}

export function useDeleteHuggingFaceTokenMutation() {
  const queryClient = useQueryClient();

  return useMutation<void, Error, void>({
    mutationFn: async () => {
      const result = await VaultAPI.deleteHuggingFaceToken();
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: HF_TOKEN_QUERY_KEY });
    },
    retry: false,
  });
}
