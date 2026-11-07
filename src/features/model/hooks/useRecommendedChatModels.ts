import { queryOptions, useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { VaultAPI, type ApiClient } from '@/lib/api';

export const RECOMMENDED_CHAT_MODELS_QUERY_KEY = ['model', 'recommended-chat-models'] as const;

/**
 * The backend's ranked list for this computer. Read again on every opening,
 * since the machine's free memory and the catalog can change between them.
 */
export const recommendedChatModelsQueryOptions = (api: ApiClient = VaultAPI) =>
  queryOptions({
    queryKey: RECOMMENDED_CHAT_MODELS_QUERY_KEY,
    queryFn: async () => {
      const result = await api.recommendChatModels();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    retry: false,
    staleTime: 0,
  });

export function useRecommendedChatModels(enabled = true) {
  const api = useApi();
  return useQuery({ ...recommendedChatModelsQueryOptions(api), enabled });
}
