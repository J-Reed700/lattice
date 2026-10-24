import { useQuery } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { LoadedModelDto, ModelRef } from '@/lib/bindings';
import { sameModelRef } from '@/utils/chatModelSelection';

export const LOADED_MODELS_QUERY_KEY = ['loaded-models'] as const;
export const DEGRADED_RECHECK_MS = 15_000;

async function loadLoadedModels(): Promise<LoadedModelDto[]> {
  const result = await VaultAPI.listLoadedModels();
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLoadedModels() {
  const query = useQuery<LoadedModelDto[]>({
    queryKey: LOADED_MODELS_QUERY_KEY,
    queryFn: loadLoadedModels,
    staleTime: 15_000,
    refetchInterval: (query) => (query.state.data?.some((entry) => entry.degraded) ? DEGRADED_RECHECK_MS : false),
  });
  return query.data ?? EMPTY;
}

const EMPTY: LoadedModelDto[] = [];

export function degradedReasonFor(model: ModelRef | null | undefined, loaded: LoadedModelDto[]): string | null {
  if (!model) return null;
  return loaded.find((entry) => entry.degraded && sameModelRef(entry.model, model))?.degraded ?? null;
}
