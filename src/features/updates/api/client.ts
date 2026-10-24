import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, UpdateInfo, VersionInfo } from '@/types';

export const updatesApi = {
  getVersionInfo: async (): Promise<ApiResult<VersionInfo>> =>
    apiCall<Wire.VersionInfoDto>('get_version_info'),

  checkForUpdates: async (): Promise<ApiResult<UpdateInfo>> =>
    apiCall<Wire.UpdateInfoDto>('check_for_updates'),
};
