import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  SystemCapabilities,
  ModelSearchResult,
  ModelRecommendation,
  ModelCatalogCacheStats,
} from '@/types';
import type { DownloadModelResponse } from '@/types/download';
import type { DownloadedModel } from '@/types/downloadedModels';
import type { SearchModelCatalogRequest } from '@/types/modelCatalog';

export const modelApi = {
  /**
   * Gets all downloaded models with metadata.
   * Includes both chat and embedding models from the downloaded_models table.
   *
   * NOTE: Gateway Pattern returns structured data directly (no JSON string parsing needed).
   *
   * @returns Array of downloaded model records
   */
  getDownloadedModels: async (): Promise<ApiResult<DownloadedModel[]>> =>
    apiCall<Wire.DownloadedModelResponse[]>('get_models_with_metadata'),

  /**
   * Checks if a model is already downloaded.
   * Queries the downloaded_models table by model_id.
   *
   * @param modelId - Model identifier to check
   * @returns True if model exists in database
   */
  isModelDownloaded: async (modelId: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('is_model_already_downloaded', { modelId }),

  /**
   * Sets the active chat model.
   * Only one model can be active for chat at a time.
   *
   * @param modelId - Model identifier to set as active
   * @returns Void on success
   */
  setActiveChatModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_chat_model', { modelId }),

  /**
   * Warms up the currently active chat model by preloading it into memory.
   * Useful after switching models so the first chat response is faster.
   *
   * @returns Void on success
   */
  warmUpActiveChatModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('warm_up_active_chat_model'),

  /**
   * Eagerly load the currently active utility model into memory.
   *
   * Utility models (HyDE expansion, intent routing, follow-up classifier)
   * pay a 60-120s cold-start cost on first use. Calling this from the
   * UI pays that cost up-front so the user's first chat turn doesn't
   * carry it. No-op (returns Ok) when no utility model is configured.
   */
  warmUpActiveUtilityModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('warm_up_active_utility_model'),

  /**
   * Gets the currently active chat model.
   * Returns null if no model is set as active.
   *
   * @returns Active chat model or null
   */
  getActiveChatModel: async (): Promise<ApiResult<DownloadedModel | null>> =>
    apiCall<Wire.DownloadedModelResponse | null>('get_active_chat_model'),

  /**
   * Sets the active embedding model.
   * Only one model can be active for embeddings at a time.
   *
   * @param modelId - Model identifier to set as active
   * @returns Void on success
   */
  setActiveEmbeddingModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_embedding_model', { modelId }),

  /**
   * Sets the active utility model (used for HyDE, routing, intent classification).
   * Only one model can be active for utility at a time.
   *
   * @param modelId - Model identifier to set as active utility model
   * @returns Void on success
   */
  setActiveUtilityModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_utility_model', { modelId }),

  /**
   * Clears the active utility model — reverts HyDE/router back to the chat model.
   *
   * @returns Void on success
   */
  clearActiveUtilityModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_utility_model'),

  /**
   * Gets the currently active embedding model.
   * Returns null if no model is set as active.
   *
   * @returns Active embedding model or null
   */
  getActiveEmbeddingModel: async (): Promise<
    ApiResult<DownloadedModel | null>
  > =>
    apiCall<Wire.DownloadedModelResponse | null>('get_active_embedding_model'),

  /**
   * Absolute path of the local models folder (created on first call).
   */
  getModelDownloadPath: async (): Promise<ApiResult<string>> =>
    apiCall<string>('get_model_download_path'),

  getActiveModels: async (): Promise<
    ApiResult<{
      chat_model: DownloadedModel | null;
      embedding_model: DownloadedModel | null;
    }>
  > => apiCall<Wire.ActiveModels>('get_active_models'),

  /**
   * Deletes a downloaded model record and optionally its file.
   * Removes from database and optionally from disk.
   *
   * @param modelId - Model identifier to delete
   * @param deleteFile - Whether to delete the file from disk (default: false)
   * @returns Void on success
   */
  deleteDownloadedModel: async (
    modelId: string,
    deleteFile: boolean = false,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('delete_downloaded_model_and_file', { modelId, deleteFile }),

  /**
   * Gets system hardware capabilities for model selection.
   * Detects GPU, RAM, CPU cores, and platform to recommend compatible models.
   *
   * @returns System capabilities including GPU availability and resources
   */
  getSystemCapabilities: async (): Promise<ApiResult<SystemCapabilities>> =>
    apiCall<Wire.SystemCapabilitiesResponse>('detect_system_capabilities'),

  /**
   * Downloads a model from the catalog.
   * Starts a download job that can be tracked via download management.
   *
   * @param modelId - Model identifier to download
   * @returns Download ID for tracking progress
   */
  downloadModel: async (
    modelId: string,
  ): Promise<ApiResult<DownloadModelResponse>> =>
    apiCall<Wire.DownloadModelResponse>('download_model', { modelId }),

  /**
   * Deletes a model file from disk.
   * Removes the model file but may leave database record.
   *
   * @param modelId - Model identifier to delete
   * @returns Void on success
   */
  deleteModel: async (
    modelId: string,
    deleteFile = true,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('delete_model', { modelId, deleteFile }),

  /**
   * Detects system capabilities for model compatibility.
   * Similar to getSystemCapabilities but may include more detailed info.
   *
   * @returns Detailed system capabilities
   */
  detectSystemCapabilities: async (): Promise<ApiResult<SystemCapabilities>> =>
    apiCall<Wire.SystemCapabilitiesResponse>('detect_system_capabilities'),

  /**
   * Gets all models compatible with the current system.
   * Filters catalog by RAM, GPU, and other requirements.
   *
   * @returns Array of compatible models
   */
  getCompatibleModels: async (
    category?: string,
  ): Promise<ApiResult<ModelRecommendation[]>> =>
    apiCall<Wire.ModelRecommendationDto[]>(
      'get_compatible_models',
      category ? { category } : {},
    ),

  /**
   * Gets all recommended models across all tasks.
   * Returns best options for chat, embedding, etc.
   *
   * @returns Array of recommended models by task type
   */
  getAllRecommendedModels: async (): Promise<
    ApiResult<ModelRecommendation[]>
  > => apiCall<Wire.ModelRecommendationDto[]>('get_all_recommended_models'),

  /**
   * Searches the model catalog by name or description.
   * Supports fuzzy matching for model discovery.
   *
   * @param query - Search query string
   * @returns Array of matching models with relevance scores
   */
  searchModelCatalog: async (
    request: SearchModelCatalogRequest,
  ): Promise<ApiResult<ModelSearchResult[]>> =>
    apiCall<Wire.ModelSearchResultDto[]>('search_model_catalog', { request }),

  getModelVariants: async (repoId: string): Promise<ApiResult<Wire.ModelMetadataDto[]>> =>
    apiCall<Wire.ModelMetadataDto[]>('get_model_variants', { repoId }),

  /**
   * Refreshes the model catalog from remote source.
   * Updates available models and metadata from HuggingFace.
   *
   * @returns Void on success
   */
  refreshModelCatalog: async (): Promise<ApiResult<void>> =>
    apiCall<void>('refresh_model_catalog'),

  /**
   * Clears the cached model catalog data.
   * Forces next request to fetch fresh catalog.
   *
   * @returns Void on success
   */
  clearModelCatalogCache: async (): Promise<ApiResult<number>> =>
    apiCall<number>('clear_model_catalog_cache'),

  /**
   * Gets statistics about the model catalog cache.
   * Returns cache usage metrics and effectiveness.
   *
   * @returns Cache statistics
   */
  getModelCatalogStats: async (): Promise<ApiResult<ModelCatalogCacheStats>> =>
    apiCall<Wire.ModelCatalogStats>('get_model_catalog_stats'),

  /**
   * Clears the active chat model selection.
   * Deactivates any currently selected chat model.
   *
   * @returns Void on success
   */
  clearActiveChatModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_chat_model'),

  /**
   * Clears the active embedding model selection.
   * Deactivates any currently selected embedding model.
   *
   * @returns Void on success
   */
  clearActiveEmbeddingModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_embedding_model'),

  /**
   * Asks whether this machine still needs the first-run model bundle.
   *
   * @returns The status as a JSON string (kept small for the backend future)
   */
  checkFirstRunStatus: async (): Promise<ApiResult<string>> =>
    apiCall<string>('check_first_run_status'),

  /**
   * Starts the first-run download of the recommended embedding model.
   *
   * @returns The backend's start message
   */
  downloadDefaultEmbeddingModel: async (): Promise<ApiResult<string>> =>
    apiCall<string>('download_default_embedding_model'),
};
