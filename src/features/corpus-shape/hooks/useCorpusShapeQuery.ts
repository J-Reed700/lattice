import { useQuery } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { CorpusShapeDto } from '@/types';

export const CORPUS_SHAPE_QUERY_KEY = ['corpus', 'shape'] as const;

/**
 * Vault-wide type mix and recent growth.
 *
 * For surfaces that do not already hold the document list — Home and the
 * post-ingest sentence. The Library counts its own array instead.
 */
export function useCorpusShapeQuery() {
  return useQuery<CorpusShapeDto>({
    queryKey: CORPUS_SHAPE_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getCorpusShape();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 30_000,
  });
}
