import type * as Wire from '@/lib/bindings';
import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandName } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';

const studyCall = <T>(command: CommandName, args?: Record<string, unknown>) =>
  studyActivityCall<T>('study', command, args);

export const studyApi = {
  /** Builds a comparison table across documents. */
  listStudyDecks: (): Promise<ApiResult<Wire.StudyDeckSummaryDto[]>> =>
    studyCall('list_study_decks'),

  getStudyDeck: (id: string): Promise<ApiResult<Wire.StudyDeckDto>> =>
    studyCall('get_study_deck', { id }),

  generateStudyDeck: (
    request: Wire.GenerateStudyDeckRequestDto,
  ): Promise<ApiResult<Wire.StudyDeckDto>> =>
    studyCall('generate_study_deck', { request }),

  generateConversationStudyDeck: (
    request: Wire.GenerateConversationStudyDeckRequestDto,
  ): Promise<ApiResult<Wire.StudyDeckDto>> =>
    studyCall('generate_conversation_study_deck', { request }),

  reviewStudyCard: (
    request: Wire.ReviewStudyCardRequestDto,
  ): Promise<ApiResult<Wire.StudyCardDto>> =>
    studyCall('review_study_card', { request }),

  updateStudyCard: (
    request: Wire.UpdateStudyCardRequestDto,
  ): Promise<ApiResult<void>> => studyCall('update_study_card', { request }),

  deleteStudyDeck: (id: string): Promise<ApiResult<void>> =>
    studyCall('delete_study_deck', { id }),
};
