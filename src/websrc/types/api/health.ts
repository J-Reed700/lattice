/**
 * Health Check API Types
 *
 * Type definitions for system health monitoring.
 * These types match the Rust backend structures from commands/health.rs
 */

export type HealthStatus = import('../../lib/bindings').HealthStatus;

export interface ComponentHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  message: string | null;
  latency_ms: number | null;
}
