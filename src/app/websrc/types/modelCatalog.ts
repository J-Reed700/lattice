/**
 * Model Catalog Type Definitions
 *
 * Type system matching Rust backend domain models for model discovery,
 * compatibility scoring, and catalog search.
 */

// ============================================================================
// Enumerations (re-exported from leaf file to avoid cycle with api/models.ts)
// ============================================================================

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

// ============================================================================
// Value Objects
// ============================================================================

/**
 * System hardware capabilities detected from the user's machine.
 *
/**
 * Complete metadata for an AI model.
 *
 * Contains all information needed for compatibility scoring,
 * download management, performance estimation, and user presentation.
 */
export interface ModelMetadata {
  /** Unique model identifier (e.g., "phi-3-mini-4k-instruct-q4") */
  id: string;
  /** Human-readable name (e.g., "Phi-3 Mini") */
  name: string;
  /** Model category (LLM, Embedding, OCR) */
  category: ModelCategory;
  /** Detailed description of model capabilities */
  description: string;
  /** Model size in gigabytes (on disk) */
  size_gb: number;
  /** Minimum RAM required to load model */
  minimum_ram_gb: number;
  /** Recommended RAM for good performance */
  recommended_ram_gb: number;
  /** Maximum context length in tokens */
  context_length: number;
  /** Performance characteristic */
  performance_tier: PerformanceTier;
  /** Available quantization formats (e.g., Q4_K_M, Q5_K_M, F16) */
  supported_quantizations: string[];
  /** Model capabilities (e.g., "chat", "code", "reasoning") */
  capabilities: string[];
  /** Download URL if available */
  download_url: string | null;
  /** License (e.g., "MIT", "Apache-2.0") */
  license: string;
  /** Whether this model requires authentication to download */
  requires_auth: boolean;
  /** HuggingFace model ID (e.g., "microsoft/Phi-3-mini-4k-instruct-gguf") */
  model_id: string | null;
  /** Default GGUF filename for this model */
  default_filename: string | null;
  /** Files for multi-file models */
  files: Array<{
    filename: string;
    size_bytes: number;
    checksum: string | null;
    url: string;
    downloaded: boolean;
  }>;
  /** Total size in bytes */
  total_size_bytes: number;
  /** Output dimension for embedding models (e.g., 384, 768, 1024). Null for non-embedding models. */
  embedding_dimensions: number | null;
  /** Compatibility verdict for embedding models. Null for non-embedding models. */
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

/**
 * On-disk model format. Matches Rust ModelFormat enum (lowercase serde
 * serialization).
 */
export type ModelFormat = 'gguf' | 'safetensors';

/**
 * Whether the local CandleEmbeddingService can actually load a given
 * embedding model. Threaded through from the backend so the catalog UI
 * can disable + badge incompatible rows before users start a multi-GB
 * download that would fail at model-load time.
 */
export type EmbeddingCompatibility =
  | { kind: 'compatible'; architecture: string }
  | { kind: 'incompatible'; architecture: string; reason: string }
  | { kind: 'unknown' };

/**
 * Detailed compatibility analysis between a model and system.
 *
 * Provides overall compatibility level, detailed factor scores,
 * performance estimates, and recommendations.
 */
export interface CompatibilityScore {
  /** Overall compatibility level */
  compatibility_level: CompatibilityLevel;
  /** Overall compatibility score (0-100) */
  overall_score: number;
  /** RAM compatibility score (0-100) */
  ram_score: number;
  /** GPU compatibility score (0-100) */
  gpu_score: number;
  /** Disk space compatibility score (0-100) */
  disk_score: number;
  /** Estimated tokens per second (for LLMs) */
  estimated_tokens_per_second: number | null;
  /** Estimated model loading time in seconds */
  estimated_loading_time_seconds: number;
  /** Human-readable recommendations */
  recommendations: string[];
  /** Blockers preventing model from running */
  blockers: string[];
}

/**
 * Model recommendation with compatibility analysis.
 *
 * Combines model metadata with compatibility score and ranking.
 */
export interface ModelRecommendation {
  /** Model metadata */
  model: ModelMetadata;
  /** Compatibility analysis */
  compatibility: CompatibilityScore;
  /** Ranking score (0-100) combining compatibility and model quality */
  ranking_score: number;
  /** Upstream popularity metric (download count), when available */
  popularity_downloads?: number | null;
  /** Upstream popularity metric (like count), when available */
  popularity_likes?: number | null;
}

// ============================================================================
// Search Filters
// ============================================================================

/**
 * Search filters for model catalog queries.
 *
 * Encapsulates search criteria for finding models:
 * - Category filtering (LLM, Embedding, OCR)
 * - Size constraints (max_size_gb)
 * - Required capabilities (chat, code, etc.)
 * - Text query matching (name, description)
 */
export interface SearchFilters {
  /** Filter by model category (LLM, Embedding, OCR) */
  category: ModelCategory | null;
  /** Maximum model size in gigabytes */
  max_size_gb: number | null;
  /** Minimum download count to filter obscure models */
  min_downloads: number | null;
  /** Required capabilities (e.g., "chat", "code") */
  required_capabilities: string[];
  /** Text query to match against name/description */
  query_text: string | null;
  /** Filter embedding models by output dimension (e.g., 768). Null = no filter. */
  embedding_dimensions: number | null;
}

// ============================================================================
// Search Results
// ============================================================================

/**
 * Model search result with relevance score.
 *
 * Combines model metadata with relevance score and source indicator.
 */
export interface ModelSearchResult {
  /** Model metadata */
  model: ModelMetadata;
  /** Relevance score (0-100) based on query match */
  relevance_score: number;
  /** Source of the model (Curated or External) */
  source: ModelSource;
  /** Upstream popularity metric (download count), when available */
  popularity_downloads?: number | null;
  /** Upstream popularity metric (like count), when available */
  popularity_likes?: number | null;
}

// ============================================================================
// Cache Statistics
// ============================================================================

/**
 * Cache statistics for model catalog.
 *
 * Tracks cache usage and effectiveness.
 */
export interface CacheStats {
  /** Total number of cache entries */
  total_entries: number;
  /** Number of valid (non-expired) entries */
  valid_entries: number;
  /** Number of expired entries */
  expired_entries: number;
}

// ============================================================================
// Request/Response DTOs
// ============================================================================

/**
 * Request to search external model catalogs.
 *
 * Re-exported from bindings to ensure type compatibility with backend.
 */
export type { SearchModelCatalogRequest } from '../lib/bindings';

/**
 * Result from compatible model search.
 */
export interface CompatibleModelResult {
  /** Models that are compatible with the system */
  compatible_models: ModelRecommendation[];
  /** System capabilities used for scoring */
  system_capabilities: import('./api/models').SystemCapabilities;
}
