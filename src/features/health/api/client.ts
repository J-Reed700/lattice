import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, HealthStatus, SystemStats } from '@/types';

export const healthApi = {
  initializeDatabase: async (): Promise<ApiResult<string>> =>
    apiCall<string>('initialize_database'),

  healthCheck: async (): Promise<ApiResult<HealthStatus>> =>
    apiCall<Wire.HealthStatus>('health_check'),

  getSystemStats: async (): Promise<ApiResult<SystemStats>> =>
    apiCall<Wire.SystemStats>('get_system_stats'),

  getVersion: async (): Promise<ApiResult<string>> =>
    apiCall<string>('get_version'),

  getBackendLogs: async (limit?: number): Promise<ApiResult<Wire.BackendLogEntry[]>> =>
    apiCall<Wire.BackendLogEntry[]>('get_backend_logs', { limit: limit ?? null }),
};
