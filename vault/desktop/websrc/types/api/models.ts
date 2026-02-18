/**
 * Model Management API Types
 *
 * Type definitions for model management, downloads, and catalog operations.
 */

import type { CpuArchitecture, GpuType, GpuAcceleration } from '../modelCatalog';

export interface SystemCapabilities {
  /** Total system RAM in gigabytes */
  total_ram_gb: number;
  /** Number of CPU cores */
  cpu_cores: number;
  /** CPU architecture (ARM64, x86_64) */
  cpu_architecture: CpuArchitecture;
  /** GPU type if present */
  gpu_type: GpuType;
  /** GPU acceleration framework available */
  gpu_acceleration: GpuAcceleration;
  /** VRAM in gigabytes (if detectable) */
  vram_gb: number | null;
  /** Available disk space in gigabytes */
  available_disk_gb: number;
}

export interface ModelInfo {
  /** Unique model identifier (e.g., "phi-3-mini") */
  id: string;
  /** Human-readable model name (e.g., "Phi-3 Mini") */
  name: string;
  /** Model size in gigabytes */
  size_gb: number;
  /** Maximum context length in tokens */
  context_length: number;
  /** Model capabilities (e.g., ["chat", "code", "reasoning"]) */
  capabilities: string[];
  /** Minimum RAM required to run model (in GB) */
  minimum_ram_gb: number;
}

export interface ModelCatalogStats {
  /** Total number of models in catalog */
  total_models: number;
  /** Number of embedding models */
  embedding_models: number;
  /** Number of chat models */
  chat_models: number;
  /** Last catalog refresh timestamp (ISO 8601) */
  last_updated: string;
  /** Catalog version */
  version: string;
}

export interface ModelSearchResult {
  /** Matching models */
  models: ModelInfo[];
  /** Total matching count (for pagination) */
  total: number;
}
