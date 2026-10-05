import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, UpdateInfo, VersionInfo } from '@/types';

export const updatesApi = {
  /**
   * Gets detailed version information including build metadata.
   * Returns comprehensive version info for troubleshooting and support.
   *
   * @returns Detailed version information object
   */
  getVersionInfo: async (): Promise<ApiResult<VersionInfo>> =>
    apiCall<Wire.VersionInfoDto>('get_version_info'),

  /**
   * Checks for application updates.
   * Queries the update server for newer versions.
   *
   * @returns Update information with available version and download URL
   */
  checkForUpdates: async (): Promise<ApiResult<UpdateInfo>> =>
    apiCall<Wire.UpdateInfoDto>('check_for_updates'),
};
