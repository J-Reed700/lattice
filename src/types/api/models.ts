/**
 * Model Management API Types
 *
 * Type definitions for model management, downloads, and catalog operations.
 */


export type SystemCapabilities = import('../../lib/bindings').SystemCapabilitiesResponse;

export type ModelInfo = import('../../lib/bindings').ModelInfo;

export type ModelCatalogStats = import('../../lib/bindings').ModelCatalogStats;

export interface ModelSearchResult {
  /** Matching models */
  models: ModelInfo[];
  /** Total matching count (for pagination) */
  total: number;
}
