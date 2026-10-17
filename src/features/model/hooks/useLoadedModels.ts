import { useQuery } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { LoadedModelDto, ModelRef } from '@/lib/bindings';
import { sameModelRef } from '@/utils/chatModelSelection';

/**
 * The models loaded right now. Invalidated when a turn pins its model, which
 * is when a model may just have loaded, or loaded on the CPU, and when a role
 * finishes warming up (useModelWarmupListener).
 */
export const LOADED_MODELS_QUERY_KEY = ['loaded-models'] as const;

/**
 * How often the list is read again while a model in it runs on the CPU. An
 * unloaded or evicted model announces nothing, and its "on CPU" must not
 * outlive it.
 */
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

/**
 * Why `model` is running on the CPU, when a loaded instance of it is; `null`
 * when it is not loaded or runs on the GPU.
 */
export function degradedReasonFor(model: ModelRef | null | undefined, loaded: LoadedModelDto[]): string | null {
  if (!model) return null;
  return loaded.find((entry) => entry.degraded && sameModelRef(entry.model, model))?.degraded ?? null;
}
