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
  getDownloadedModels: async (): Promise<ApiResult<DownloadedModel[]>> =>
    apiCall<Wire.DownloadedModelResponse[]>('list_downloaded_models'),

  isModelDownloaded: async (modelId: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('is_model_already_downloaded', { modelId }),

  setActiveChatModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_chat_model', { modelId }),

  warmUpActiveChatModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('warm_up_active_chat_model'),

  warmUpActiveUtilityModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('warm_up_active_utility_model'),

  warmUpLearningModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('warm_up_learning_model'),

  listLoadedModels: async (): Promise<ApiResult<Wire.LoadedModelDto[]>> =>
    apiCall<Wire.LoadedModelDto[]>('list_loaded_models'),

  lastLearningWindowShortfall: async (): Promise<ApiResult<Wire.LearningWindowShortfallDto | null>> =>
    apiCall<Wire.LearningWindowShortfallDto | null>('last_learning_window_shortfall'),

  getActiveChatModel: async (): Promise<ApiResult<DownloadedModel | null>> =>
    apiCall<Wire.DownloadedModelResponse | null>('get_active_chat_model'),

  setActiveEmbeddingModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_embedding_model', { modelId }),

  setActiveUtilityModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_utility_model', { modelId }),

  clearActiveUtilityModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_utility_model'),

  getActiveEmbeddingModel: async (): Promise<
    ApiResult<DownloadedModel | null>
  > =>
    apiCall<Wire.DownloadedModelResponse | null>('get_active_embedding_model'),

  getModelDownloadPath: async (): Promise<ApiResult<string>> =>
    apiCall<string>('get_model_download_path'),

  getActiveModels: async (): Promise<
    ApiResult<{
      chat_model: DownloadedModel | null;
      embedding_model: DownloadedModel | null;
    }>
  > => apiCall<Wire.ActiveModels>('get_active_models'),

  deleteDownloadedModel: async (
    modelId: string,
    deleteFile: boolean = false,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('delete_model', { modelId, deleteFile }),

  getSystemCapabilities: async (): Promise<ApiResult<SystemCapabilities>> =>
    apiCall<Wire.SystemCapabilitiesResponse>('detect_system_capabilities'),

  downloadModel: async (
    modelId: string,
  ): Promise<ApiResult<DownloadModelResponse>> =>
    apiCall<Wire.DownloadModelResponse>('download_model', { modelId }),

  deleteModel: async (
    modelId: string,
    deleteFile = true,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('delete_model', { modelId, deleteFile }),

  detectSystemCapabilities: async (): Promise<ApiResult<SystemCapabilities>> =>
    apiCall<Wire.SystemCapabilitiesResponse>('detect_system_capabilities'),

  getCompatibleModels: async (
    category?: string,
  ): Promise<ApiResult<ModelRecommendation[]>> =>
    apiCall<Wire.ModelRecommendationDto[]>(
      'get_compatible_models',
      category ? { category } : {},
    ),

  getAllRecommendedModels: async (): Promise<
    ApiResult<ModelRecommendation[]>
  > => apiCall<Wire.ModelRecommendationDto[]>('get_all_recommended_models'),

  searchModelCatalog: async (
    request: SearchModelCatalogRequest,
  ): Promise<ApiResult<ModelSearchResult[]>> =>
    apiCall<Wire.ModelSearchResultDto[]>('search_model_catalog', { request }),

  getModelVariants: async (repoId: string): Promise<ApiResult<Wire.ModelMetadataDto[]>> =>
    apiCall<Wire.ModelMetadataDto[]>('get_model_variants', { repoId }),

  refreshModelCatalog: async (): Promise<ApiResult<void>> =>
    apiCall<void>('refresh_model_catalog'),

  clearModelCatalogCache: async (): Promise<ApiResult<number>> =>
    apiCall<number>('clear_model_catalog_cache'),

  getModelCatalogStats: async (): Promise<ApiResult<ModelCatalogCacheStats>> =>
    apiCall<Wire.ModelCatalogStats>('get_model_catalog_stats'),

  clearActiveChatModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_chat_model'),

  clearActiveEmbeddingModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_embedding_model'),

  checkFirstRunStatus: async (): Promise<ApiResult<string>> =>
    apiCall<string>('check_first_run_status'),

  downloadDefaultEmbeddingModel: async (): Promise<ApiResult<string>> =>
    apiCall<string>('download_default_embedding_model'),

  /** Lists unneeded files left in the model folder by legacy safetensors downloads. */
  listLeftoverEmbeddingFiles: async (): Promise<ApiResult<Wire.LeftoverEmbeddingFilesDto>> =>
    apiCall<Wire.LeftoverEmbeddingFilesDto>('list_leftover_embedding_files'),

  removeLeftoverEmbeddingFiles: async (): Promise<ApiResult<Wire.LeftoverEmbeddingFilesDto>> =>
    apiCall<Wire.LeftoverEmbeddingFilesDto>('remove_leftover_embedding_files'),
};
