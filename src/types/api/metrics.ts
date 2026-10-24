export interface MetricsSnapshot {
  documentsIndexed: number;
  searchesPerformed: number;
  qaQueries: number;
  cacheHits: number;
  cacheMisses: number;
  avgSearchTimeMs: number;
  avgQaTimeMs: number;
  uptimeSeconds: number;
}

export type SystemStats = import('../../lib/bindings').SystemStats;
