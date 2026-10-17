import { useQuery } from '@tanstack/react-query';

import { modelApi } from '@/features/model/api/client';

/** The downloadable files of one catalog repository. A failure is shown, not retried. */
export function useModelVariantsQuery(repoId: string) {
  return useQuery({
    queryKey: ['model-catalog', 'variants', repoId],
    queryFn: async () => {
      const result = await modelApi.getModelVariants(repoId);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 5 * 60_000,
    retry: false,
  });
}
