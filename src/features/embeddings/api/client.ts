import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, EmbeddingModelInfo } from '@/types';

export const embeddingsApi = {
  /**
   * Generates a vector embedding for a text string.
   * Uses ONNX model to convert text into semantic vector representation.
   *
   * @param text - Text to convert to embedding
   * @returns Embedding as array of numbers (typically 384 or 768 dimensions)
   */
  generateEmbedding: async (text: string): Promise<ApiResult<number[]>> =>
    apiCall<number[]>('generate_embedding', { text }),

  /**
   * Generates embeddings for multiple texts in a single batch.
   * More efficient than calling generateEmbedding repeatedly.
   *
   * @param texts - Array of text strings to embed
   * @returns Array of embeddings (2D array of numbers)
   */
  generateEmbeddingsBatch: async (
    texts: string[],
  ): Promise<ApiResult<number[][]>> =>
    apiCall<number[][]>('generate_embeddings_batch', { texts }),

  /**
   * Gets information about the loaded embedding model.
   * Returns model name, dimensions, and metadata.
   *
   * @returns Model information object
   */
  getEmbeddingModelInfo: async (): Promise<ApiResult<EmbeddingModelInfo>> =>
    apiCall<Wire.ModelInfo>('get_embedding_model_info'),
};
