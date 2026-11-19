import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, HfTokenStatus } from '@/types';

export const huggingfaceApi = {
  /**
   * Stores a HuggingFace authentication token securely in OS keyring.
   * Token must start with "hf_" and be at least 35 characters.
   *
   * @param token - HuggingFace API token
   * @returns Void on success
   */
  setHuggingFaceToken: async (token: string): Promise<ApiResult<null>> =>
    apiCall('set_huggingface_token', { token }),

  /**
   * Gets HuggingFace token status (whether it's set).
   * Does not return the actual token value for security.
   *
   * @returns Token status object
   */
  getHuggingFaceTokenStatus: async (): Promise<ApiResult<HfTokenStatus>> =>
    apiCall('get_huggingface_token_status'),

  /**
   * Deletes the stored HuggingFace token from OS keyring.
   *
   * @returns Void on success
   */
  deleteHuggingFaceToken: async (): Promise<ApiResult<null>> =>
    apiCall('delete_huggingface_token'),
};
