import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, HealthStatus, SystemStats } from '@/types';

export const healthApi = {
  /**
   * Initializes the SQLite database with required schema and tables.
   * Creates all necessary tables for documents, embeddings, tags, and metadata.
   * Should be called once on application startup.
   *
   * @returns Success message confirming database initialization
   */
  initializeDatabase: async (): Promise<ApiResult<string>> =>
    apiCall<string>('initialize_database'),

  /**
   * Performs a health check on the backend system (DDD architecture).
   * Verifies database connectivity, model availability, and system status.
   *
   * @returns Health status object with component statuses
   * Uses the current domain command.
   */
  healthCheck: async (): Promise<ApiResult<HealthStatus>> =>
    apiCall<Wire.HealthStatus>('health_check'),

  /**
   * Retrieves system statistics (DDD architecture).
   * Includes total documents, chunks, tags, and storage size.
   *
   * @returns System statistics object
   * Uses the current domain command.
   */
  getSystemStats: async (): Promise<ApiResult<SystemStats>> =>
    apiCall<Wire.SystemStats>('get_system_stats'),

  /**
   * Gets the application version from Cargo.toml.
   * Returns semantic version string for display and update checking.
   *
   * @returns Version string (e.g., "0.1.0")
   */
  getVersion: async (): Promise<ApiResult<string>> =>
    apiCall<string>('get_version'),
};
