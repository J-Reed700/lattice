import { queryOptions, useMutation } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { VaultAPI, type ApiClient } from '@/lib/api';
import type { ApiResult } from '@/types';

export interface RecommendedModel {
  model_id: string;
  display_name: string;
  estimated_size_bytes: number;
}

export interface FirstRunStatus {
  needs_setup: boolean;
  // Legacy fields kept for transitional safety
  recommended_model_id: string | null;
  recommended_model_name: string | null;
  estimated_size_bytes: number | null;
  // New shape
  embedding_model: RecommendedModel | null;
  chat_model: RecommendedModel | null;
  total_estimated_size_bytes: number | null;
}

/**
 * Whether this machine still needs models, and which ones. The command
 * returns a JSON string to keep its Future small (stack overflow fix), so a
 * malformed answer rejects.
 */
export async function fetchFirstRunStatus(api: ApiClient = VaultAPI): Promise<ApiResult<FirstRunStatus>> {
  const result = await api.checkFirstRunStatus();
  return result.ok ? { ok: true, data: JSON.parse(result.data) as FirstRunStatus } : result;
}

export const FIRST_RUN_STATUS_QUERY_KEY = ['model', 'first-run-status'] as const;

/**
 * The status as a query, shared by the gate that decides to offer the bundle
 * and the dialog that shows it. Each opening reads it again behind what was
 * read last; a failed read is reported at once rather than retried.
 */
export const firstRunStatusQueryOptions = (api: ApiClient = VaultAPI) =>
  queryOptions({
    queryKey: FIRST_RUN_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await fetchFirstRunStatus(api);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    retry: false,
    staleTime: 0,
  });

/**
 * Starts the first-run downloads. Embedding uses its dedicated first-run
 * command; chat uses the generic download path. Each answers with the
 * command's result, and progress arrives through the downloads listener.
 */
export function useFirstRunDownloads() {
  const api = useApi();
  const embedding = useMutation({ mutationFn: () => api.downloadDefaultEmbeddingModel() });
  const chat = useMutation({ mutationFn: (modelId: string) => api.downloadModel(modelId) });
  return { downloadEmbeddingModel: embedding.mutateAsync, downloadChatModel: chat.mutateAsync };
}
