import { useMutation } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
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
export async function fetchFirstRunStatus(): Promise<ApiResult<FirstRunStatus>> {
  const result = await VaultAPI.checkFirstRunStatus();
  return result.ok ? { ok: true, data: JSON.parse(result.data) as FirstRunStatus } : result;
}

/**
 * Starts the first-run downloads. Embedding uses its dedicated first-run
 * command; chat uses the generic download path. Each answers with the
 * command's result, and progress arrives through the downloads listener.
 */
export function useFirstRunDownloads() {
  const embedding = useMutation({ mutationFn: () => VaultAPI.downloadDefaultEmbeddingModel() });
  const chat = useMutation({ mutationFn: (modelId: string) => VaultAPI.downloadModel(modelId) });
  return { downloadEmbeddingModel: embedding.mutateAsync, downloadChatModel: chat.mutateAsync };
}
