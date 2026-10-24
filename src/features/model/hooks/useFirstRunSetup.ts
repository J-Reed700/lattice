import { queryOptions, useMutation } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { ApiResult } from '@/types';

export interface RecommendedModel {
  model_id: string;
  display_name: string;
  estimated_size_bytes: number;
}

export interface FirstRunStatus {
  needs_setup: boolean;
  recommended_model_id: string | null;
  recommended_model_name: string | null;
  estimated_size_bytes: number | null;
  embedding_model: RecommendedModel | null;
  chat_model: RecommendedModel | null;
  total_estimated_size_bytes: number | null;
}

export async function fetchFirstRunStatus(): Promise<ApiResult<FirstRunStatus>> {
  const result = await VaultAPI.checkFirstRunStatus();
  return result.ok ? { ok: true, data: JSON.parse(result.data) as FirstRunStatus } : result;
}

export const FIRST_RUN_STATUS_QUERY_KEY = ['model', 'first-run-status'] as const;

export const firstRunStatusQueryOptions = () =>
  queryOptions({
    queryKey: FIRST_RUN_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await fetchFirstRunStatus();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    retry: false,
    staleTime: 0,
  });

export function useFirstRunDownloads() {
  const embedding = useMutation({ mutationFn: () => VaultAPI.downloadDefaultEmbeddingModel() });
  const chat = useMutation({ mutationFn: (modelId: string) => VaultAPI.downloadModel(modelId) });
  return { downloadEmbeddingModel: embedding.mutateAsync, downloadChatModel: chat.mutateAsync };
}
