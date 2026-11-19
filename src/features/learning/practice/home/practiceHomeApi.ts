import { newId } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandArgList, CommandName } from '@/shared/ipc/transport';
import { unwrapApiResult } from '@/types/api/result';

async function call<C extends CommandName>(command: C, ...args: CommandArgList<C>): Promise<Wire.CommandResults[C]> {
  return unwrapApiResult(await studyActivityCall('learning', command, ...args));
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

  codingSessions: (programId: string): Promise<Wire.PracticeSessionSummaryDto[]> =>
    call('list_practice_sessions', { request: { programId } }),

  /** A Coached session on a coding activity that has none yet; returns its id. */
  startPracticeSession: async (programId: string, activityId: string): Promise<string> => {
    const request: Wire.StartPracticeSessionRequestDto = { operationId: newId(), programId, activityId, setting: 'coached', budgetSeconds: null };
    const session = await call('start_practice_session', { request });
    return session.sessionId;
  },
};
