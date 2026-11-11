import { unwrap } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { apiCall, type CommandName } from '@/shared/ipc/transport';

async function call<T>(command: CommandName, args: { request: unknown }): Promise<T> {
  return unwrap(await apiCall<T>(command, args));
}

/** One wrapper per coding command; each throws the backend's message when it fails. */
export const codingApi = {
  startBuild: (request: Wire.StartPracticeBuildRequestDto) =>
    call<Wire.StartPracticeBuildResultDto>('start_practice_build', { request }),
  startSession: (request: Wire.StartPracticeSessionRequestDto) =>
    call<Wire.PracticeSessionDto>('start_practice_session', { request }),
  getSession: (request: Wire.PracticeSessionRequestDto) =>
    call<Wire.PracticeSessionDto>('get_practice_session', { request }),
  listSessions: (request: Wire.ListPracticeSessionsRequestDto) =>
    call<Wire.PracticeSessionSummaryDto[]>('list_practice_sessions', { request }),
  moveStage: (request: Wire.MovePracticeStageRequestDto) =>
    call<Wire.PracticeSessionDto>('move_practice_stage', { request }),
  checkExample: (request: Wire.CheckCodingExampleRequestDto) =>
    call<Wire.PracticeSessionDto>('check_coding_example', { request }),
  removeExample: (request: Wire.RemoveCodingExampleRequestDto) =>
    call<Wire.PracticeSessionDto>('remove_coding_example', { request }),
  run: (request: Wire.CheckPracticePieceRequestDto) =>
    call<Wire.PracticeSessionDto>('check_practice_piece', { request }),
  askHint: (request: Wire.AskPracticeHintRequestDto) =>
    call<Wire.PracticeSessionDto>('ask_practice_hint', { request }),
  sendTurn: (request: Wire.SendPracticeTurnRequestDto) =>
    call<Wire.PracticeSessionDto>('send_practice_turn', { request }),
  respond: (request: Wire.PracticeSessionRequestDto) =>
    call<Wire.PracticeSessionDto>('respond_practice_session', { request }),
  observeDraft: (request: Wire.ObserveCodingDraftRequestDto) =>
    call<Wire.PracticeSessionDto>('observe_coding_draft', { request }),
  setClock: (request: Wire.SetPracticeClockRequestDto) =>
    call<Wire.ClockDto>('set_practice_clock', { request }),
  setSetting: (request: Wire.SetPracticeSettingRequestDto) =>
    call<Wire.PracticeSessionDto>('set_practice_setting', { request }),
  revealSolution: (request: Wire.RevealPracticeSolutionRequestDto) =>
    call<Wire.PracticeSessionDto>('reveal_practice_solution', { request }),
  finish: (request: Wire.FinishPracticeSessionRequestDto) =>
    call<Wire.PracticeSessionDto>('finish_practice_session', { request }),

  /** The revision of the activity whose draft the learner's code is saved under. */
  async activityRevision(programId: string, activityId: string): Promise<number> {
    const workspace = unwrap(
      await apiCall<Wire.LearningPracticalWorkspaceDto>('get_learning_practical_workspace', { programId }),
    );
    const activity = workspace.activities.find((candidate) => candidate.id === activityId);
    if (!activity) throw new Error('This problem is no longer in the course.');
    return activity.revision;
  },
};
