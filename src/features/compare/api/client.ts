import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';
import type {
  CompareDocumentsRequest,
  CompareTableDto,
} from '@/types/api/compare';

export const compareApi = {
  compareDocuments: async (
    request: CompareDocumentsRequest,
  ): Promise<ApiResult<CompareTableDto>> =>
    apiCall<CompareTableDto>('compare_documents', { request }),
};
