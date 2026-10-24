export type SystemCapabilities = import('../../lib/bindings').SystemCapabilitiesResponse;

export type ModelInfo = import('../../lib/bindings').ModelInfo;

export type ModelCatalogStats = import('../../lib/bindings').ModelCatalogStats;

export interface ModelSearchResult {
  models: ModelInfo[];
  total: number;
}
