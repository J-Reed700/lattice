import type * as Wire from '@/lib/bindings';
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
  setHuggingFaceToken: async (token: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_huggingface_token', { token }),

  /**
   * Gets HuggingFace token status (whether it's set).
   * Does not return the actual token value for security.
   *
   * @returns Token status object
   */
  getHuggingFaceTokenStatus: async (): Promise<ApiResult<HfTokenStatus>> =>
    apiCall<Wire.HfTokenStatus>('get_huggingface_token_status'),

  /**
   * Gets the actual HuggingFace token value.
   * Should only be used internally for download authentication.
   *
   * @returns Token string or null if not set
   */
  getHuggingFaceToken: async (): Promise<ApiResult<string | null>> =>
    apiCall<string | null>('get_huggingface_token'),

  /**
   * Deletes the stored HuggingFace token from OS keyring.
   *
   * @returns Void on success
   */
  deleteHuggingFaceToken: async (): Promise<ApiResult<void>> =>
    apiCall<void>('delete_huggingface_token'),
};
