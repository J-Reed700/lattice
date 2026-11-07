import { useQueries } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import type { ApiClient } from '@/lib/api';
import type { FitVerdictDto, ModelFileRefDto } from '@/lib/bindings';
import type { ModelMetadata } from '@/types/modelCatalog';

/** What a row shows for its fit: a verdict, `null` while its size is read, `'unknown'` when it could not be, `undefined` for no label. */
export type ShownFit = FitVerdictDto | 'unknown' | null | undefined;

const fitKey = (ref: ModelFileRefDto) => ['model', 'fit', ref.repo, ref.file, ref.revision ?? null] as const;

interface Pending {
  ref: ModelFileRefDto;
  waiters: Array<{ resolve: (fit: FitVerdictDto | null) => void; reject: (error: Error) => void }>;
}

/**
 * Collects every load made in one tick and sends them as one request. Each
 * load resolves with its own answer, `null` included, or all of them reject
 * when the call fails, so a failure is an error to retry, never an answer.
 */
class FitLoader {
  private pending = new Map<string, Pending>();

  constructor(private readonly api: ApiClient) {}

  load(ref: ModelFileRefDto): Promise<FitVerdictDto | null> {
    return new Promise((resolve, reject) => {
      const id = JSON.stringify(fitKey(ref));
      const entry = this.pending.get(id) ?? { ref, waiters: [] };
      entry.waiters.push({ resolve, reject });
      if (this.pending.size === 0) setTimeout(() => void this.flush(), 0);
      this.pending.set(id, entry);
    });
  }

  private async flush(): Promise<void> {
    const batch = [...this.pending.values()];
    this.pending.clear();
    try {
      const result = await this.api.readModelFits(batch.map((entry) => entry.ref));
      if (!result.ok) throw new Error(result.error);
      batch.forEach((entry, index) => entry.waiters.forEach((waiter) => waiter.resolve(result.data[index] ?? null)));
    } catch (error) {
      const failure = error instanceof Error ? error : new Error(String(error));
      batch.forEach((entry) => entry.waiters.forEach((waiter) => waiter.reject(failure)));
    }
  }
}

const loaders = new WeakMap<ApiClient, FitLoader>();
function loaderFor(api: ApiClient): FitLoader {
  let loader = loaders.get(api);
  if (!loader) loaders.set(api, loader = new FitLoader(api));
  return loader;
}

/** The file a verdict is read for, or null when the model names no file. */
export function modelFileRef(model: Pick<ModelMetadata, 'model_id' | 'default_filename'>): ModelFileRefDto | null {
  return model.model_id && model.default_filename ? { repo: model.model_id, file: model.default_filename } : null;
}

/**
 * Answers for the files asked, in order: a verdict, `null` when the header
 * could not be read, `undefined` while unanswered or after a failed request.
 * Each file is one query, so the cache is shared and a file is asked for once;
 * a failed request is retried and asked again on a later mount.
 */
export function useModelFits(refs: ModelFileRefDto[]): Array<FitVerdictDto | null | undefined> {
  const loader = loaderFor(useApi());
  const results = useQueries({
    queries: refs.map((ref) => ({
      queryKey: fitKey(ref),
      queryFn: () => loader.load(ref),
      staleTime: Infinity,
      gcTime: Infinity,
    })),
  });
  return results.map((result) => result.data);
}

/**
 * Each entry's fit as it should be shown. A verdict that arrived with the row
 * is shown as is; a row that arrived with `null` is read through the shared
 * cache. An entry that names no file cannot be read: its size is unknown.
 */
export function useShownFits(entries: Array<{ model: ModelMetadata; fit?: FitVerdictDto | null }>): ShownFit[] {
  const refs: ModelFileRefDto[] = [];
  const slots = entries.map(({ model, fit }) => {
    if (fit !== null) return -1;
    const ref = modelFileRef(model);
    return ref ? refs.push(ref) - 1 : -2;
  });
  const answers = useModelFits(refs);
  return entries.map(({ fit }, index) => {
    const slot = slots[index];
    if (slot === -1) return fit;
    if (slot === -2) return 'unknown';
    const answer = answers[slot];
    return answer === undefined ? null : answer ?? 'unknown';
  });
}
