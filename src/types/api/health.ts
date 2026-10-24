export type HealthStatus = import('../../lib/bindings').HealthStatus;

export interface ComponentHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  message: string | null;
  latency_ms: number | null;
}
