import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';
import type {
  CreatePassageReferenceRequest,
  PassageReferenceDto,
} from '@/types/api/passageReferences';
import type { UpdatePassageReferenceRequest } from '@/types/api/references';

export const referencesApi = {
  /**
   * Saves an excerpt from a document as a reference.
   */
  createPassageReference: async (
    request: CreatePassageReferenceRequest,
  ): Promise<ApiResult<PassageReferenceDto>> =>
    apiCall('create_passage_reference', { request }),

  /** Saved passage references, newest first. */
  listPassageReferences: async (
    limit?: number,
  ): Promise<ApiResult<PassageReferenceDto[]>> =>
    apiCall('list_passage_references', { limit }),

  /** Updates a passage reference's title and note. `null` clears a field. */
  updatePassageReference: async (
    request: UpdatePassageReferenceRequest,
  ): Promise<ApiResult<PassageReferenceDto>> =>
    apiCall('update_passage_reference', { request }),

  /** Deletes a saved passage reference. */
  deletePassageReference: async (id: string): Promise<ApiResult<void>> =>
    apiCall('delete_passage_reference', { id }),
};
