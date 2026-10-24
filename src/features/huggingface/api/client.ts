import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, HfTokenStatus } from '@/types';

export const huggingfaceApi = {
  setHuggingFaceToken: async (token: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_huggingface_token', { token }),

  getHuggingFaceTokenStatus: async (): Promise<ApiResult<HfTokenStatus>> =>
    apiCall<Wire.HfTokenStatus>('get_huggingface_token_status'),

  getHuggingFaceToken: async (): Promise<ApiResult<string | null>> =>
    apiCall<string | null>('get_huggingface_token'),

  deleteHuggingFaceToken: async (): Promise<ApiResult<void>> =>
    apiCall<void>('delete_huggingface_token'),
};
