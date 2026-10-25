import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import VaultAPI, { type ApiClient } from '@/lib/api';
import type { CorpusShapeDto } from '@/types';

export const CORPUS_SHAPE_QUERY_KEY = ['corpus', 'shape'] as const;

/** Vault-wide type mix and recent growth, read now. */
export async function fetchCorpusShape(api: ApiClient = VaultAPI): Promise<CorpusShapeDto> {
  const result = await api.getCorpusShape();
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

/**
 * Vault-wide type mix and recent growth.
 *
 * For surfaces that do not already hold the document list — Home and the
 * post-ingest sentence. The Library counts its own array instead.
 */
export function useCorpusShapeQuery() {
  const api = useApi();
  return useQuery<CorpusShapeDto>({
    queryKey: CORPUS_SHAPE_QUERY_KEY,
    queryFn: () => fetchCorpusShape(api),
    staleTime: 30_000,
  });
}
