/**
 * Model Catalog Type Definitions
 *
 * Type system matching Rust backend domain models for model discovery,
 * compatibility scoring, and catalog search.
 */


import type { ModelCategory } from './modelCatalogPrimitives';

export type {
  ModelCategory,
  PerformanceTier,
  GpuType,
  GpuAcceleration,
  CpuArchitecture,
  ModelSource,
  ModelSortBy,
} from './modelCatalogPrimitives';


export type ModelMetadata = import('../lib/bindings').ModelMetadataDto;

export type EmbeddingCompatibility = import('../lib/bindings').EmbeddingCompatibilityDto;

export type ModelRecommendation = import('../lib/bindings').ModelRecommendationDto;


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


export type ModelSearchResult = import('../lib/bindings').ModelSearchResultDto;


export type CacheStats = import('../lib/bindings').ModelCatalogStats;


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
