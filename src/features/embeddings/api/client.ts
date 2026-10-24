import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, EmbeddingModelInfo } from '@/types';

export const embeddingsApi = {
  generateEmbedding: async (text: string): Promise<ApiResult<number[]>> =>
    apiCall<number[]>('generate_embedding', { text }),

  generateEmbeddingsBatch: async (
    texts: string[],
  ): Promise<ApiResult<number[][]>> =>
    apiCall<number[][]>('generate_embeddings_batch', { texts }),

  getEmbeddingModelInfo: async (): Promise<ApiResult<EmbeddingModelInfo>> =>
    apiCall<Wire.ModelInfo>('get_embedding_model_info'),
};
