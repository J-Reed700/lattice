import type * as Wire from '@/lib/bindings';
import { newId } from '@/lib/api';
import { studyActivityCall } from '@/lib/studyActivity';
import { unwrapApiResult } from '@/types/api/result';
import type { CommandName } from '@/shared/ipc/transport';

async function call<T>(command: string, request: Record<string, unknown>): Promise<T> {
  return unwrapApiResult(await studyActivityCall<T>('learning', command as CommandName, { request }));
}

/** The Practice page's commands: the course's profile, practice fit, what is due, and its coding sessions. */
export const practiceHomeApi = {
  getProfile: (programId: string): Promise<Wire.LearningPracticeProfileDto> =>
    call('get_learning_practice_profile', { programId }),

  saveProfile: (request: Wire.SaveLearningPracticeProfileRequestDto): Promise<Wire.LearningPracticeProfileDto> =>
    call('save_learning_practice_profile', request),

  getFit: (programId: string): Promise<Wire.PracticeFitDto> => call('get_practice_fit', { programId }),

  checkFit: (programId: string): Promise<Wire.PracticeFitDto> => call('check_practice_fit', { programId }),

  due: (programId: string): Promise<Wire.PracticeDueDto> => call('practice_due', { programId }),

  codingSessions: (programId: string): Promise<Wire.CodingSessionSummaryDto[]> =>
    call('list_coding_sessions', { programId }),

  /** A Coached session on a coding activity that has none yet; returns its id. */
  startCodingSession: async (programId: string, activityId: string): Promise<string> => {
    const session = await call<Wire.CodingSessionDto>('start_coding_session', {
      operationId: newId(),
      programId,
      activityId,
      setting: 'coached',
      budgetSeconds: null,
    });
    return session.sessionId;
  },
};
