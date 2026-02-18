/**
 * Health Check API Types
 *
 * Type definitions for system health monitoring.
 * These types match the Rust backend structures from commands/health.rs
 */

export interface HealthStatus {
  status: 'healthy' | 'degraded' | 'unhealthy';
  database: ComponentHealth;
  embedding_service: ComponentHealth;
  qa_engine: ComponentHealth;
  cache: ComponentHealth;
  search_index: ComponentHealth;
}

export interface ComponentHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  message: string | null;
  latency_ms: number | null;
}
