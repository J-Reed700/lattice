import type {
  ModelCategory,
  PerformanceTier,
  CompatibilityLevel,
  ModelSource,
} from './modelCatalogPrimitives';

export type {
  ModelCategory,
  PerformanceTier,
  GpuType,
  GpuAcceleration,
  CpuArchitecture,
  CompatibilityLevel,
  ModelSource,
  ModelSortBy,
} from './modelCatalogPrimitives';

export interface ModelMetadata {
  id: string;
  name: string;
  category: ModelCategory;
  description: string;
  size_gb: number;
  minimum_ram_gb: number;
  recommended_ram_gb: number;
  context_length: number;
  performance_tier: PerformanceTier;
  supported_quantizations: string[];
  capabilities: string[];
  download_url: string | null;
  license: string;
  requires_auth: boolean;
  model_id: string | null;
  default_filename: string | null;
  files: Array<{
    filename: string;
    size_bytes: number;
    checksum: string | null;
    url: string;
    downloaded: boolean;
  }>;
  total_size_bytes: number;
  embedding_dimensions: number | null;
  embedding_compatibility: EmbeddingCompatibility | null;
  /**
   * On-disk storage format. Drives which mistralrs builder is used at
   * load time. Backwards-compat default = 'gguf' if missing.
   * - 'gguf': single-file quantized blob (llama.cpp ecosystem)
   * - 'safetensors': HF directory with config.json + sharded weights;
   *   loaded via mistralrs::ModelBuilder which auto-detects text vs
   *   multimodal. Unquantized — significantly larger on disk + RAM.
   */
  format?: ModelFormat;
}

export type ModelFormat = 'gguf' | 'safetensors';

/**
 * Whether the bundled llama-server can actually serve a given
 * embedding model. Threaded through from the backend so the catalog UI
 * can disable + badge incompatible rows before users start a multi-GB
 * download that would fail at model-load time.
 */
export type EmbeddingCompatibility =
  | { kind: 'compatible'; architecture: string }
  | { kind: 'incompatible'; architecture: string; reason: string }
  | { kind: 'unknown' };

export interface CompatibilityScore {
  compatibility_level: CompatibilityLevel;
  overall_score: number;
  ram_score: number;
  gpu_score: number;
  disk_score: number;
  estimated_tokens_per_second: number | null;
  estimated_loading_time_seconds: number;
  recommendations: string[];
  blockers: string[];
}

export interface ModelRecommendation {
  model: ModelMetadata;
  compatibility: CompatibilityScore;
  ranking_score: number;
  popularity_downloads?: number | null;
  popularity_likes?: number | null;
}

export interface SearchFilters {
  category: ModelCategory | null;
  max_size_gb: number | null;
  min_downloads: number | null;
  required_capabilities: string[];
  query_text: string | null;
  embedding_dimensions: number | null;
}

export interface ModelSearchResult {
  model: ModelMetadata;
  relevance_score: number;
  source: ModelSource;
  popularity_downloads?: number | null;
  popularity_likes?: number | null;
}

export interface CacheStats {
  total_entries: number;
  valid_entries: number;
  expired_entries: number;
}

export type { SearchModelCatalogRequest } from '../lib/bindings';

export interface CompatibleModelResult {
  compatible_models: ModelRecommendation[];
  system_capabilities: import('./api/models').SystemCapabilities;
}
