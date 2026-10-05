import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';

export const studyApi = {
  /** Builds a comparison table across documents. */
  listStudyDecks: (): Promise<ApiResult<Wire.StudyDeckSummaryDto[]>> =>
    apiCall('list_study_decks'),

  getStudyDeck: (id: string): Promise<ApiResult<Wire.StudyDeckDto>> =>
    apiCall('get_study_deck', { id }),

  generateStudyDeck: (
    request: Wire.GenerateStudyDeckRequestDto,
  ): Promise<ApiResult<Wire.StudyDeckDto>> =>
    apiCall('generate_study_deck', { request }),

  generateConversationStudyDeck: (
    request: Wire.GenerateConversationStudyDeckRequestDto,
  ): Promise<ApiResult<Wire.StudyDeckDto>> =>
    apiCall('generate_conversation_study_deck', { request }),

  reviewStudyCard: (
    request: Wire.ReviewStudyCardRequestDto,
  ): Promise<ApiResult<Wire.StudyCardDto>> =>
    apiCall('review_study_card', { request }),

  updateStudyCard: (
    request: Wire.UpdateStudyCardRequestDto,
  ): Promise<ApiResult<void>> => apiCall('update_study_card', { request }),

  deleteStudyDeck: (id: string): Promise<ApiResult<void>> =>
    apiCall('delete_study_deck', { id }),
};
