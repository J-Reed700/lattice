import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import type { ModelRef } from '@/lib/bindings';
import { modelRefKey } from '@/utils/chatModelSelection';

/** What a connected model's provider reports it can take. */
export function useModelLimits(model: ModelRef) {
  const api = useApi();
  return useQuery({
    queryKey: ['model-limits', modelRefKey(model)],
    queryFn: async () => {
      const result = await api.getModelLimits(model);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 5 * 60_000,
    retry: false,
  });
}
