export interface MetricDto {
  name: string;
  value: number;
  unit: string | null;
  timestamp: string;
}

export interface MetricsSnapshotDto {
  documentsIndexed: number;
  searchesPerformed: number;
  qaQueries: number;

  cacheHits: number;
  cacheMisses: number;

  avgSearchTimeMs: number;
  avgQaTimeMs: number;

  uptimeSeconds: number;
}

export interface RecordMetricResultDto {
  success: boolean;
  message: string | null;
}

export interface CacheStats {
  totalAccesses: number;
  hits: number;
  misses: number;
  hitRatePercent: number;
}

export function calculateCacheStats(metrics: MetricsSnapshotDto): CacheStats {
  const totalAccesses = metrics.cacheHits + metrics.cacheMisses;
  const hitRatePercent = totalAccesses > 0
    ? (metrics.cacheHits / totalAccesses) * 100
    : 0;

  return {
    totalAccesses,
    hits: metrics.cacheHits,
    misses: metrics.cacheMisses,
    hitRatePercent,
  };
}

export function formatUptime(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m ${secs}s`;
  } else if (minutes > 0) {
    return `${minutes}m ${secs}s`;
  } else {
    return `${secs}s`;
  }
}
