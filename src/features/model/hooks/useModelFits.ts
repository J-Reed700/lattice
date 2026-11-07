import { useEffect } from 'react';

import { useQueries, useQueryClient, type QueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import type { FitVerdictDto, ModelFileRefDto } from '@/lib/bindings';
import type { ModelMetadata } from '@/types/modelCatalog';

/** What a row shows for its fit: a verdict, `null` while its size is read, `'unknown'` when it could not be, `undefined` for no label. */
export type ShownFit = FitVerdictDto | 'unknown' | null | undefined;

const fitKey = (ref: ModelFileRefDto) => ['model', 'fit', ref.repo, ref.file, ref.revision ?? null] as const;
const inFlight = new WeakMap<QueryClient, Set<string>>();

/** The file a verdict is read for, or null when the model names no file. */
export function modelFileRef(model: Pick<ModelMetadata, 'model_id' | 'default_filename'>): ModelFileRefDto | null {
  return model.model_id && model.default_filename ? { repo: model.model_id, file: model.default_filename } : null;
}

/**
 * Answers for the files asked, in order: a verdict, `null` when the header
 * could not be read, `undefined` while unanswered. Files already answered or
 * in flight anywhere in the app are not asked for again; the rest go in one
 * request.
 */
export function useModelFits(refs: ModelFileRefDto[]): Array<FitVerdictDto | null | undefined> {
  const api = useApi();
  const queryClient = useQueryClient();
  const results = useQueries({
    queries: refs.map((ref) => ({
      queryKey: fitKey(ref),
      queryFn: (): Promise<FitVerdictDto | null> => Promise.reject(new Error('A size is read in a batch, not alone.')),
      enabled: false,
      gcTime: Infinity,
    })),
  });
  const signature = JSON.stringify(refs.map(fitKey));

  useEffect(() => {
    let flying = inFlight.get(queryClient);
    if (!flying) inFlight.set(queryClient, flying = new Set());
    const asked = refs.filter((ref) => {
      const id = JSON.stringify(fitKey(ref));
      return queryClient.getQueryData(fitKey(ref)) === undefined && !flying.has(id);
    });
    if (asked.length === 0) return;
    const ids = asked.map((ref) => JSON.stringify(fitKey(ref)));
    ids.forEach((id) => flying.add(id));
    void api.readModelFits(asked)
      .then((result) => (result.ok ? result.data : []))
      .catch(() => [] as Array<FitVerdictDto | null>)
      .then((answers) => {
        asked.forEach((ref, index) => queryClient.setQueryData(fitKey(ref), answers[index] ?? null));
        ids.forEach((id) => flying.delete(id));
      });
    // `signature` stands for `refs`, which is a new array every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, queryClient, signature]);

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
