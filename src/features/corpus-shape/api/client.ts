import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, ClusterDto, ClusterRunDto } from '@/types';

export const corpusShapeApi = {
  /** The themes from the most recent clustering run. */
  listClusters: async (): Promise<ApiResult<ClusterDto[]>> =>
    apiCall<Wire.ClusterDto[]>('list_clusters'),

  /** Runs clustering over the vault and replaces the stored themes. */
  clusterVaultRun: async (): Promise<ApiResult<ClusterRunDto>> =>
    apiCall<Wire.ClusterRunDto>('cluster_vault_run'),
};
