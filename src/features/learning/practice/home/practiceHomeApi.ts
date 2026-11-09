import { newId } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandName } from '@/shared/ipc/transport';
import { unwrapApiResult } from '@/types/api/result';

async function call<T>(command: CommandName, args: Record<string, unknown>): Promise<T> {
  return unwrapApiResult(await studyActivityCall<T>('learning', command, args));
}

/** The Practice page's commands: the course's profile, practice fit, what is due, and its coding sessions. */
export const practiceHomeApi = {
  getProfile: (programId: string): Promise<Wire.LearningPracticeProfileDto> =>
    call('get_learning_practice_profile', { request: { programId } }),

  saveProfile: (request: Wire.SaveLearningPracticeProfileRequestDto): Promise<Wire.LearningPracticeProfileDto> =>
    call('save_learning_practice_profile', { request }),

  getFit: (programId: string): Promise<Wire.PracticeFitDto> => call('get_practice_fit', { request: { programId } }),

  checkFit: (programId: string): Promise<Wire.PracticeFitDto> => call('check_practice_fit', { request: { programId } }),

  due: (programId: string): Promise<Wire.PracticeDueDto> => call('practice_due', { request: { programId } }),

  codingSessions: (programId: string): Promise<Wire.CodingSessionSummaryDto[]> =>
    call('list_coding_sessions', { request: { programId } }),

  /** A Coached session on a coding activity that has none yet; returns its id. */
  startCodingSession: async (programId: string, activityId: string): Promise<string> => {
    const session = await call<Wire.CodingSessionDto>('start_coding_session', {
      request: { operationId: newId(), programId, activityId, setting: 'coached', budgetSeconds: null },
    });
    return session.sessionId;
  },
};
