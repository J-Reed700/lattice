import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';
import type {
  CreatePassageReferenceRequest,
  PassageReferenceDto,
} from '@/types/api/passageReferences';
import type { UpdatePassageReferenceRequest } from '@/types/api/references';

export const referencesApi = {
  createPassageReference: async (
    request: CreatePassageReferenceRequest,
  ): Promise<ApiResult<PassageReferenceDto>> =>
    apiCall<PassageReferenceDto>('create_passage_reference', { request }),

  listPassageReferences: async (
    limit?: number,
  ): Promise<ApiResult<PassageReferenceDto[]>> =>
    apiCall<PassageReferenceDto[]>('list_passage_references', { limit }),

  updatePassageReference: async (
    request: UpdatePassageReferenceRequest,
  ): Promise<ApiResult<PassageReferenceDto>> =>
    apiCall<PassageReferenceDto>('update_passage_reference', { request }),

  deletePassageReference: async (id: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_passage_reference', { id }),
};
