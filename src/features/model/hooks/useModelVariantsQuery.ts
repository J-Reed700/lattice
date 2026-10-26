import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';

/** The downloadable files of one catalog repository. A failure is shown, not retried. */
export function useModelVariantsQuery(repoId: string) {
  const api = useApi();
  return useQuery({
    queryKey: ['model-catalog', 'variants', repoId],
    queryFn: async () => {
      const result = await api.getModelVariants(repoId);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 5 * 60_000,
    retry: false,
  });
}
